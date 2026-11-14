#include "ffmpeg_bridge.h"

typedef struct GcSegmentTime {
    int64_t start_us;
    int64_t end_us;
} GcSegmentTime;

typedef struct GcMuxStreamState {
    uint32_t active_segment;
    int64_t segment_first_dts;
    int64_t output_offset;
    int64_t last_dts;
    int64_t last_duration;
    int64_t next_output_offset;
} GcMuxStreamState;

static char *gc_segments_strdup(const char *text) {
    if (!text) {
        return NULL;
    }
    size_t len = strlen(text) + 1;
    char *copy = (char *)malloc(len);
    if (copy) {
        memcpy(copy, text, len);
    }
    return copy;
}

static void gc_set_segment_error(GcExportResult *result, const char *prefix, int code) {
    if (!result || result->error) {
        return;
    }
    char detail[AV_ERROR_MAX_STRING_SIZE] = {0};
    if (code < 0) {
        av_strerror(code, detail, sizeof(detail));
    }
    char message[1024] = {0};
    if (code < 0 && detail[0] != '\0') {
        snprintf(message, sizeof(message), "%s: %s", prefix, detail);
    } else {
        snprintf(message, sizeof(message), "%s", prefix);
    }
    result->error = gc_segments_strdup(message);
}

static int64_t gc_packet_time_us(AVPacket *packet, AVStream *stream) {
    int64_t timestamp = packet->dts != AV_NOPTS_VALUE ? packet->dts : packet->pts;
    if (timestamp == AV_NOPTS_VALUE) {
        return AV_NOPTS_VALUE;
    }
    return av_rescale_q(timestamp, stream->time_base, AV_TIME_BASE_Q);
}

static int64_t gc_packet_duration_us(AVPacket *packet, AVStream *stream) {
    if (packet->duration <= 0) {
        return 0;
    }
    return av_rescale_q(packet->duration, stream->time_base, AV_TIME_BASE_Q);
}

static void gc_mux_state_init(GcMuxStreamState *state) {
    state->active_segment = UINT32_MAX;
    state->segment_first_dts = AV_NOPTS_VALUE;
    state->output_offset = 0;
    state->last_dts = INT64_MIN;
    state->last_duration = 0;
    state->next_output_offset = 0;
}

static int gc_retimestamp_packet_for_segment(
    AVPacket *packet,
    GcMuxStreamState *state,
    uint32_t segment_index,
    AVRational input_time_base,
    AVRational output_time_base
) {
    int64_t original_dts = packet->dts;
    int64_t original_pts = packet->pts;
    int64_t original_duration = packet->duration;
    int64_t segment_base = original_dts != AV_NOPTS_VALUE ? original_dts : original_pts;
    if (segment_base == AV_NOPTS_VALUE) {
        segment_base = 0;
    }

    if (state->active_segment != segment_index) {
        state->active_segment = segment_index;
        state->segment_first_dts = segment_base;
        state->output_offset = state->next_output_offset;
    }

    int64_t new_dts = AV_NOPTS_VALUE;
    int64_t new_pts = AV_NOPTS_VALUE;
    if (original_dts != AV_NOPTS_VALUE) {
        new_dts = av_rescale_q(original_dts - state->segment_first_dts, input_time_base, output_time_base) +
            state->output_offset;
    }
    if (original_pts != AV_NOPTS_VALUE) {
        new_pts = av_rescale_q(original_pts - state->segment_first_dts, input_time_base, output_time_base) +
            state->output_offset;
    }
    if (new_dts == AV_NOPTS_VALUE && new_pts != AV_NOPTS_VALUE) {
        new_dts = new_pts;
    }
    if (new_pts == AV_NOPTS_VALUE && new_dts != AV_NOPTS_VALUE) {
        new_pts = new_dts;
    }
    if (new_dts == AV_NOPTS_VALUE) {
        new_dts = state->last_dts == INT64_MIN
            ? state->output_offset
            : state->last_dts + (state->last_duration > 0 ? state->last_duration : 1);
        new_pts = new_dts;
    }
    if (new_dts != AV_NOPTS_VALUE && new_dts <= state->last_dts) {
        int64_t delta = state->last_dts + 1 - new_dts;
        new_dts += delta;
        if (new_pts != AV_NOPTS_VALUE) {
            new_pts += delta;
        }
    }
    if (new_pts != AV_NOPTS_VALUE && new_dts != AV_NOPTS_VALUE && new_pts < new_dts) {
        new_pts = new_dts;
    }
    packet->dts = new_dts;
    packet->pts = new_pts;
    packet->duration = original_duration > 0
        ? FFMAX(av_rescale_q(original_duration, input_time_base, output_time_base), 1)
        : 0;
    return 0;
}

static AVRational gc_stream_frame_rate(AVStream *stream) {
    if (stream->avg_frame_rate.num > 0 && stream->avg_frame_rate.den > 0) {
        return stream->avg_frame_rate;
    }
    if (stream->r_frame_rate.num > 0 && stream->r_frame_rate.den > 0) {
        return stream->r_frame_rate;
    }
    return (AVRational){30, 1};
}

static int64_t gc_frame_duration_in_time_base(AVRational frame_rate, AVRational time_base) {
    if (frame_rate.num <= 0 || frame_rate.den <= 0 || time_base.num <= 0 || time_base.den <= 0) {
        return 1;
    }
    int64_t duration = av_rescale_q(1, (AVRational){frame_rate.den, frame_rate.num}, time_base);
    return duration > 0 ? duration : 1;
}

static int gc_retimestamp_packet_cfr_video(
    AVPacket *packet,
    GcMuxStreamState *state,
    int64_t frame_duration,
    AVRational input_time_base,
    AVRational output_time_base
) {
    frame_duration = frame_duration > 0 ? frame_duration : 1;
    int64_t pts_offset = 0;
    if (packet->pts != AV_NOPTS_VALUE && packet->dts != AV_NOPTS_VALUE) {
        pts_offset = av_rescale_q(packet->pts - packet->dts, input_time_base, output_time_base);
        if (pts_offset < 0) {
            pts_offset = 0;
        }
    }

    int64_t dts = state->last_dts == INT64_MIN ? 0 : state->last_dts + frame_duration;
    packet->dts = dts;
    packet->pts = dts + pts_offset;
    packet->duration = frame_duration;
    return 0;
}

static void gc_update_mux_state_after_packet(AVPacket *packet, GcMuxStreamState *state) {
    if (packet->duration > 0) {
        state->last_duration = packet->duration;
    }
    if (packet->dts != AV_NOPTS_VALUE) {
        state->last_dts = packet->dts;
    } else if (state->last_dts == INT64_MIN) {
        state->last_dts = 0;
    } else {
        state->last_dts += state->last_duration > 0 ? state->last_duration : 1;
    }

    int64_t duration = packet->duration > 0 ? packet->duration : 1;
    if (packet->dts != AV_NOPTS_VALUE) {
        int64_t end = packet->dts + duration;
        if (end > state->next_output_offset) {
            state->next_output_offset = end;
        }
    }
    /* The next segment continues the decode clock. PTS already includes the
       B-frame reorder delay; using its maximum adds that delay at every cut. */
}

static int gc_open_export_input(const char *path, AVFormatContext **format_out, int *video_stream_out) {
    AVFormatContext *format = NULL;
    int ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        return ret;
    }

    ret = avformat_find_stream_info(format, NULL);
    if (ret < 0) {
        avformat_close_input(&format);
        return ret;
    }

    int video_stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        avformat_close_input(&format);
        return video_stream;
    }

    *format_out = format;
    *video_stream_out = video_stream;
    return 0;
}

static int gc_codecpar_compatible(const AVCodecParameters *expected, const AVCodecParameters *actual) {
    if (!expected || !actual) {
        return 0;
    }
    if (expected->codec_type != actual->codec_type ||
        expected->codec_id != actual->codec_id ||
        expected->format != actual->format ||
        expected->profile != actual->profile ||
        expected->level != actual->level ||
        expected->extradata_size != actual->extradata_size) {
        return 0;
    }
    if (expected->codec_type == AVMEDIA_TYPE_VIDEO &&
        (expected->width != actual->width || expected->height != actual->height)) {
        return 0;
    }
    if (expected->codec_type == AVMEDIA_TYPE_AUDIO &&
        (expected->sample_rate != actual->sample_rate ||
         expected->ch_layout.nb_channels != actual->ch_layout.nb_channels)) {
        return 0;
    }
    if (expected->extradata_size > 0 &&
        memcmp(expected->extradata, actual->extradata, (size_t)expected->extradata_size) != 0) {
        return 0;
    }
    return 1;
}

static int gc_build_segment_time(
    AVFormatContext *input_format,
    int video_stream,
    const GcPacketRange *range,
    GcSegmentTime *segment
) {
    segment->start_us = AV_NOPTS_VALUE;
    segment->end_us = AV_NOPTS_VALUE;

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        return AVERROR(ENOMEM);
    }

    AVStream *stream = input_format->streams[video_stream];
    int ret = 0;
    int64_t video_packet_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        if (packet->stream_index != video_stream) {
            av_packet_unref(packet);
            continue;
        }
        if (video_packet_index > range->end_packet) {
            av_packet_unref(packet);
            break;
        }
        if (video_packet_index >= range->start_packet) {
            int64_t packet_time = gc_packet_time_us(packet, stream);
            if (segment->start_us == AV_NOPTS_VALUE) {
                segment->start_us = packet_time;
            }
            if (packet_time != AV_NOPTS_VALUE) {
                int64_t duration = gc_packet_duration_us(packet, stream);
                segment->end_us = packet_time + (duration > 0 ? duration : 1);
            }
        }
        video_packet_index += 1;
        av_packet_unref(packet);
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }
    av_packet_free(&packet);
    if (ret < 0) {
        return ret;
    }
    if (segment->start_us == AV_NOPTS_VALUE || segment->end_us == AV_NOPTS_VALUE) {
        return AVERROR(EINVAL);
    }
    if (segment->end_us <= segment->start_us) {
        segment->end_us = segment->start_us + 1;
    }
    ret = av_seek_frame(input_format, -1, 0, AVSEEK_FLAG_BACKWARD);
    if (ret < 0) {
        ret = avformat_seek_file(input_format, -1, INT64_MIN, 0, INT64_MAX, 0);
    }
    return ret;
}

static int gc_audio_ordinal_for_stream(AVFormatContext *format, int stream_index) {
    int ordinal = 0;
    for (unsigned int i = 0; i < format->nb_streams; i++) {
        AVStream *stream = format->streams[i];
        if (stream && stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            if ((int)i == stream_index) {
                return ordinal;
            }
            ordinal += 1;
        }
    }
    return -1;
}

static int gc_audio_stream_for_ordinal(AVFormatContext *format, int ordinal) {
    int current = 0;
    for (unsigned int i = 0; i < format->nb_streams; i++) {
        AVStream *stream = format->streams[i];
        if (stream && stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            if (current == ordinal) {
                return (int)i;
            }
            current += 1;
        }
    }
    return -1;
}

static int gc_audio_stream_count(AVFormatContext *format) {
    int count = 0;
    for (unsigned int i = 0; i < format->nb_streams; i++) {
        AVStream *stream = format->streams[i];
        if (stream && stream->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            count += 1;
        }
    }
    return count;
}

static int gc_create_output_stream_from(
    AVFormatContext *output_format,
    AVStream *input_stream,
    AVRational time_base
) {
    AVStream *output_stream = avformat_new_stream(output_format, NULL);
    if (!output_stream) {
        return AVERROR(ENOMEM);
    }
    int ret = avcodec_parameters_copy(output_stream->codecpar, input_stream->codecpar);
    if (ret < 0) {
        return ret;
    }
    output_stream->codecpar->codec_tag = 0;
    output_stream->time_base = time_base;
    output_stream->avg_frame_rate = input_stream->avg_frame_rate;
    output_stream->r_frame_rate = input_stream->r_frame_rate;
    return output_stream->index;
}

static int gc_write_segment_packets(
    AVFormatContext *input_format,
    int video_stream,
    AVFormatContext *output_format,
    int output_video_stream,
    const int *output_audio_streams,
    int audio_count,
    const GcExportInputSegment *segment,
    const GcSegmentTime *segment_time,
    uint32_t segment_index,
    int generate_video_pts,
    int64_t video_pts_step,
    GcMuxStreamState *video_state,
    GcMuxStreamState *audio_states,
    GcExportResult *result
) {
    AVStream *input_video = input_format->streams[video_stream];
    AVStream *output_video = output_format->streams[output_video_stream];
    if (!gc_codecpar_compatible(output_video->codecpar, input_video->codecpar)) {
        gc_set_segment_error(result, "segment video stream parameters differ", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    for (int audio = 0; audio < audio_count; audio++) {
        int input_audio_index = gc_audio_stream_for_ordinal(input_format, audio);
        if (input_audio_index < 0) {
            gc_set_segment_error(result, "segment audio stream is missing", AVERROR(EINVAL));
            return AVERROR(EINVAL);
        }
        AVStream *input_audio = input_format->streams[input_audio_index];
        AVStream *output_audio = output_format->streams[output_audio_streams[audio]];
        if (!gc_codecpar_compatible(output_audio->codecpar, input_audio->codecpar)) {
            gc_set_segment_error(result, "segment audio stream parameters differ", AVERROR(EINVAL));
            return AVERROR(EINVAL);
        }
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        return AVERROR(ENOMEM);
    }

    int ret = 0;
    int64_t video_packet_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        int input_index = packet->stream_index;
        int output_index = -1;
        GcMuxStreamState *state = NULL;
        AVStream *input_stream = NULL;
        AVStream *output_stream = NULL;

        if (input_index == video_stream) {
            if (video_packet_index > segment->range.end_packet) {
                av_packet_unref(packet);
                ret = 0;
                break;
            }
            if (video_packet_index >= segment->range.start_packet) {
                output_index = output_video_stream;
                state = video_state;
            }
            video_packet_index += 1;
        } else if (audio_count > 0) {
            int audio = gc_audio_ordinal_for_stream(input_format, input_index);
            if (audio >= 0 && audio < audio_count) {
                int64_t packet_time = gc_packet_time_us(packet, input_format->streams[input_index]);
                if (packet_time >= segment_time->start_us && packet_time < segment_time->end_us) {
                    output_index = output_audio_streams[audio];
                    state = &audio_states[audio];
                }
            }
        }

        if (output_index < 0) {
            av_packet_unref(packet);
            continue;
        }

        input_stream = input_format->streams[input_index];
        output_stream = output_format->streams[output_index];
        if (input_index == video_stream && generate_video_pts) {
            ret = gc_retimestamp_packet_cfr_video(
                packet,
                state,
                video_pts_step,
                input_stream->time_base,
                output_stream->time_base
            );
        } else {
            ret = gc_retimestamp_packet_for_segment(
                packet,
                state,
                segment_index,
                input_stream->time_base,
                output_stream->time_base
            );
        }
        if (ret < 0) {
            av_packet_unref(packet);
            break;
        }
        gc_update_mux_state_after_packet(packet, state);
        packet->stream_index = output_stream->index;
        packet->pos = -1;
        int64_t write_pts = packet->pts;
        int64_t write_dts = packet->dts;
        int64_t write_duration = packet->duration;
        ret = av_interleaved_write_frame(output_format, packet);
        if (ret < 0) {
            char detail[512] = {0};
            snprintf(
                detail,
                sizeof(detail),
                "segment packet write failed (segment=%u input_stream=%d output_stream=%d pts=%lld dts=%lld duration=%lld tb=%d/%d)",
                segment_index + 1,
                input_index,
                output_index,
                (long long)write_pts,
                (long long)write_dts,
                (long long)write_duration,
                output_stream->time_base.num,
                output_stream->time_base.den
            );
            gc_set_segment_error(result, detail, ret);
            break;
        }
        result->packets_written += 1;
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }
    av_packet_free(&packet);
    return ret;
}

int gc_export_mp4_segments(
    const GcExportInputSegment *segments,
    uint32_t segment_count,
    const char *output_path,
    const GcExportOptions *options,
    GcExportResult *result
) {
    AVFormatContext *first_input = NULL;
    int first_video_stream = -1;
    int ret = gc_open_export_input(segments[0].input_path, &first_input, &first_video_stream);
    if (ret < 0) {
        gc_set_segment_error(result, "failed to open first segment input", ret);
        return ret;
    }

    AVStream *first_video = first_input->streams[first_video_stream];
    AVRational output_time_base = first_video->time_base;
    if (options && options->time_base_num > 0 && options->time_base_den > 0) {
        output_time_base = (AVRational){options->time_base_num, options->time_base_den};
    }
    int include_audio = options && options->include_audio;
    int audio_count = include_audio ? gc_audio_stream_count(first_input) : 0;

    AVFormatContext *output_format = NULL;
    ret = avformat_alloc_output_context2(&output_format, NULL, "mp4", output_path);
    if (ret < 0 || !output_format) {
        avformat_close_input(&first_input);
        gc_set_segment_error(result, "failed to allocate MP4 output context", ret < 0 ? ret : AVERROR(EINVAL));
        return ret < 0 ? ret : AVERROR(EINVAL);
    }

    int output_video_stream = gc_create_output_stream_from(output_format, first_video, output_time_base);
    if (output_video_stream < 0) {
        avformat_close_input(&first_input);
        avformat_free_context(output_format);
        gc_set_segment_error(result, "failed to create output video stream", output_video_stream);
        return output_video_stream;
    }

    int *output_audio_streams = NULL;
    GcMuxStreamState *audio_states = NULL;
    if (audio_count > 0) {
        output_audio_streams = (int *)malloc(sizeof(int) * (size_t)audio_count);
        audio_states = (GcMuxStreamState *)malloc(sizeof(GcMuxStreamState) * (size_t)audio_count);
        if (!output_audio_streams || !audio_states) {
            free(output_audio_streams);
            free(audio_states);
            avformat_close_input(&first_input);
            avformat_free_context(output_format);
            gc_set_segment_error(result, "failed to allocate output audio state", AVERROR(ENOMEM));
            return AVERROR(ENOMEM);
        }
        for (int audio = 0; audio < audio_count; audio++) {
            int input_audio_index = gc_audio_stream_for_ordinal(first_input, audio);
            output_audio_streams[audio] = gc_create_output_stream_from(
                output_format,
                first_input->streams[input_audio_index],
                first_input->streams[input_audio_index]->time_base
            );
            if (output_audio_streams[audio] < 0) {
                ret = output_audio_streams[audio];
                gc_set_segment_error(result, "failed to create output audio stream", ret);
                goto setup_cleanup;
            }
            gc_mux_state_init(&audio_states[audio]);
        }
    }

    if (!(output_format->oformat->flags & AVFMT_NOFILE)) {
        ret = avio_open(&output_format->pb, output_path, AVIO_FLAG_WRITE);
        if (ret < 0) {
            gc_set_segment_error(result, "failed to open output MP4 file", ret);
            goto setup_cleanup;
        }
    }

    int header_written = 0;
    ret = avformat_write_header(output_format, NULL);
    if (ret < 0) {
        gc_set_segment_error(result, "failed to write MP4 header", ret);
        goto mux_cleanup;
    }
    header_written = 1;

    int generate_video_pts = options && options->generate_pts;
    int64_t video_pts_step = options && options->pts_step > 0 ? options->pts_step : 0;
    if (generate_video_pts && video_pts_step <= 0) {
        video_pts_step = gc_frame_duration_in_time_base(
            gc_stream_frame_rate(first_video),
            output_format->streams[output_video_stream]->time_base
        );
    }

    GcMuxStreamState video_state;
    gc_mux_state_init(&video_state);
    for (uint32_t segment_index = 0; segment_index < segment_count && ret >= 0; segment_index++) {
        AVFormatContext *input_format = NULL;
        int video_stream = -1;
        if (segment_index == 0) {
            input_format = first_input;
            video_stream = first_video_stream;
            first_input = NULL;
        } else {
            ret = gc_open_export_input(segments[segment_index].input_path, &input_format, &video_stream);
            if (ret < 0) {
                gc_set_segment_error(result, "failed to open segment input", ret);
                break;
            }
        }

        GcSegmentTime segment_time;
        ret = gc_build_segment_time(input_format, video_stream, &segments[segment_index].range, &segment_time);
        if (ret < 0) {
            gc_set_segment_error(result, "failed to derive segment time range", ret);
        }
        if (ret >= 0) {
            ret = gc_write_segment_packets(
                input_format,
                video_stream,
                output_format,
                output_video_stream,
                output_audio_streams,
                audio_count,
                &segments[segment_index],
                &segment_time,
                segment_index,
                generate_video_pts,
                video_pts_step,
                &video_state,
                audio_states,
                result
            );
        }
        avformat_close_input(&input_format);
    }

mux_cleanup:
    if (header_written) {
        int trailer_ret = av_write_trailer(output_format);
        if (ret >= 0 && trailer_ret < 0) {
            ret = trailer_ret;
        }
    }
    if (!(output_format->oformat->flags & AVFMT_NOFILE) && output_format->pb) {
        result->bytes_written = avio_tell(output_format->pb);
        int close_ret = avio_closep(&output_format->pb);
        if (ret >= 0 && close_ret < 0) {
            ret = close_ret;
        }
    }

setup_cleanup:
    if (first_input) {
        avformat_close_input(&first_input);
    }
    free(output_audio_streams);
    free(audio_states);
    avformat_free_context(output_format);
    return ret;
}
