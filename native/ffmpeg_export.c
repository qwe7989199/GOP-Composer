#include "ffmpeg_bridge.h"
#include "file_io.h"

typedef struct GcExportSegment {
    int64_t start_us;
    int64_t end_us;
} GcExportSegment;

typedef struct GcMuxStreamState {
    uint32_t active_segment;
    int64_t segment_first_dts;
    int64_t output_offset;
    int64_t last_dts;
    int64_t last_duration;
} GcMuxStreamState;

int gc_export_mp4_segments(
    const GcExportInputSegment *segments,
    uint32_t segment_count,
    const char *output_path,
    const GcExportOptions *options,
    GcExportResult *result
);

static char *gc_strdup(const char *text) {
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

static void gc_set_export_error(GcExportResult *result, const char *prefix, int code) {
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
    result->error = gc_strdup(message);
}

static int gc_is_h264_or_hevc(enum AVCodecID codec_id) {
    return codec_id == AV_CODEC_ID_H264 || codec_id == AV_CODEC_ID_HEVC;
}

static const char *gc_raw_bsf_name(enum AVCodecID codec_id) {
    if (codec_id == AV_CODEC_ID_H264) {
        return "h264_mp4toannexb";
    }
    if (codec_id == AV_CODEC_ID_HEVC) {
        return "hevc_mp4toannexb";
    }
    return NULL;
}

static int gc_write_raw_packet(
    FILE *output,
    AVBSFContext *bsf,
    AVPacket *packet,
    GcExportResult *result
) {
    int ret = 0;
    if (bsf) {
        ret = av_bsf_send_packet(bsf, packet);
        if (ret < 0) {
            return ret;
        }

        while ((ret = av_bsf_receive_packet(bsf, packet)) >= 0) {
            ret = gc_write_file(output, packet->data, (size_t)packet->size);
            if (ret < 0) {
                gc_set_export_error(result, "failed to write raw output packet", ret);
                av_packet_unref(packet);
                return ret;
            }
            result->packets_written += 1;
            result->bytes_written += packet->size;
            av_packet_unref(packet);
        }
        return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
    }

    ret = gc_write_file(output, packet->data, (size_t)packet->size);
    if (ret < 0) {
        gc_set_export_error(result, "failed to write raw output packet", ret);
        return ret;
    }
    result->packets_written += 1;
    result->bytes_written += packet->size;
    return 0;
}

static int gc_flush_raw_bsf(FILE *output, AVBSFContext *bsf, GcExportResult *result) {
    if (!bsf) {
        return 0;
    }

    int ret = av_bsf_send_packet(bsf, NULL);
    if (ret < 0) {
        return ret;
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        return AVERROR(ENOMEM);
    }

    while ((ret = av_bsf_receive_packet(bsf, packet)) >= 0) {
        ret = gc_write_file(output, packet->data, (size_t)packet->size);
        if (ret < 0) {
            gc_set_export_error(result, "failed to flush raw output packet", ret);
            av_packet_free(&packet);
            return ret;
        }
        result->packets_written += 1;
        result->bytes_written += packet->size;
        av_packet_unref(packet);
    }

    av_packet_free(&packet);
    return ret == AVERROR_EOF || ret == AVERROR(EAGAIN) ? 0 : ret;
}

static int gc_validate_ranges(const GcPacketRange *ranges, uint32_t range_count) {
    if (!ranges || range_count == 0) {
        return AVERROR(EINVAL);
    }

    int64_t previous_end = -1;
    for (uint32_t i = 0; i < range_count; i++) {
        if (ranges[i].start_packet < 0 || ranges[i].end_packet < ranges[i].start_packet) {
            return AVERROR(EINVAL);
        }
        if (i > 0 && ranges[i].start_packet <= previous_end) {
            return AVERROR(EINVAL);
        }
        previous_end = ranges[i].end_packet;
    }
    return 0;
}

static int gc_advance_range(const GcPacketRange *ranges, uint32_t range_count, uint32_t *range_index, int64_t packet_index) {
    while (*range_index < range_count && packet_index > ranges[*range_index].end_packet) {
        *range_index += 1;
    }
    return *range_index < range_count;
}

static int gc_export_raw_video_ranges(
    AVFormatContext *input_format,
    int video_stream,
    const GcPacketRange *ranges,
    uint32_t range_count,
    const char *output_path,
    GcExportResult *result
) {
    AVStream *input_stream = input_format->streams[video_stream];
    enum AVCodecID codec_id = input_stream->codecpar->codec_id;
    if (!gc_is_h264_or_hevc(codec_id)) {
        gc_set_export_error(result, "raw export only supports H.264 or HEVC video", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    FILE *output = gc_fopen_utf8_write(output_path);
    if (!output) {
        int ret = gc_file_io_error();
        gc_set_export_error(result, "failed to open raw output file", ret);
        return ret;
    }

    AVBSFContext *bsf = NULL;
    const char *bsf_name = gc_raw_bsf_name(codec_id);
    if (bsf_name) {
        const AVBitStreamFilter *filter = av_bsf_get_by_name(bsf_name);
        if (!filter) {
            fclose(output);
            return AVERROR_BSF_NOT_FOUND;
        }

        int ret = av_bsf_alloc(filter, &bsf);
        if (ret < 0) {
            fclose(output);
            return ret;
        }

        ret = avcodec_parameters_copy(bsf->par_in, input_stream->codecpar);
        if (ret < 0) {
            av_bsf_free(&bsf);
            fclose(output);
            return ret;
        }
        bsf->time_base_in = input_stream->time_base;

        ret = av_bsf_init(bsf);
        if (ret < 0) {
            av_bsf_free(&bsf);
            fclose(output);
            return ret;
        }
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        av_bsf_free(&bsf);
        fclose(output);
        return AVERROR(ENOMEM);
    }

    int ret = 0;
    int64_t video_packet_index = 0;
    uint32_t range_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        if (packet->stream_index != video_stream) {
            av_packet_unref(packet);
            continue;
        }

        if (!gc_advance_range(ranges, range_count, &range_index, video_packet_index)) {
            av_packet_unref(packet);
            ret = 0;
            break;
        }

        if (video_packet_index >= ranges[range_index].start_packet) {
            ret = gc_write_raw_packet(output, bsf, packet, result);
            if (ret < 0) {
                av_packet_unref(packet);
                break;
            }
        } else {
            av_packet_unref(packet);
        }

        video_packet_index += 1;
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }
    if (ret >= 0) {
        ret = gc_flush_raw_bsf(output, bsf, result);
    }

    av_packet_free(&packet);
    av_bsf_free(&bsf);
    int close_ret = gc_close_file(output);
    if (close_ret < 0 && ret >= 0) {
        gc_set_export_error(result, "failed to close raw output file", close_ret);
        ret = close_ret;
    }
    return ret;
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

static int gc_build_export_segments(
    AVFormatContext *input_format,
    int video_stream,
    const GcPacketRange *ranges,
    uint32_t range_count,
    GcExportSegment **segments_out
) {
    GcExportSegment *segments = (GcExportSegment *)calloc(range_count, sizeof(GcExportSegment));
    if (!segments) {
        return AVERROR(ENOMEM);
    }
    for (uint32_t i = 0; i < range_count; i++) {
        segments[i].start_us = AV_NOPTS_VALUE;
        segments[i].end_us = AV_NOPTS_VALUE;
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        free(segments);
        return AVERROR(ENOMEM);
    }

    AVStream *stream = input_format->streams[video_stream];
    int ret = 0;
    int64_t video_packet_index = 0;
    uint32_t range_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        if (packet->stream_index != video_stream) {
            av_packet_unref(packet);
            continue;
        }

        if (!gc_advance_range(ranges, range_count, &range_index, video_packet_index)) {
            av_packet_unref(packet);
            break;
        }

        if (video_packet_index >= ranges[range_index].start_packet && video_packet_index <= ranges[range_index].end_packet) {
            int64_t packet_time = gc_packet_time_us(packet, stream);
            if (segments[range_index].start_us == AV_NOPTS_VALUE) {
                segments[range_index].start_us = packet_time;
            }
            if (packet_time != AV_NOPTS_VALUE) {
                int64_t duration = gc_packet_duration_us(packet, stream);
                segments[range_index].end_us = packet_time + (duration > 0 ? duration : 1);
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
        free(segments);
        return ret;
    }

    for (uint32_t i = 0; i < range_count; i++) {
        if (segments[i].start_us == AV_NOPTS_VALUE || segments[i].end_us == AV_NOPTS_VALUE) {
            free(segments);
            return AVERROR(EINVAL);
        }
        if (segments[i].end_us <= segments[i].start_us) {
            segments[i].end_us = segments[i].start_us + 1;
        }
    }

    ret = av_seek_frame(input_format, -1, 0, AVSEEK_FLAG_BACKWARD);
    if (ret < 0) {
        ret = avformat_seek_file(input_format, -1, INT64_MIN, 0, INT64_MAX, 0);
    }
    if (ret < 0) {
        free(segments);
        return ret;
    }

    *segments_out = segments;
    return 0;
}

static int gc_find_segment_for_time(const GcExportSegment *segments, uint32_t segment_count, int64_t time_us) {
    if (time_us == AV_NOPTS_VALUE) {
        return -1;
    }
    for (uint32_t i = 0; i < segment_count; i++) {
        if (time_us >= segments[i].start_us && time_us < segments[i].end_us) {
            return (int)i;
        }
    }
    return -1;
}

static void gc_mux_state_init(GcMuxStreamState *state) {
    state->active_segment = UINT32_MAX;
    state->segment_first_dts = AV_NOPTS_VALUE;
    state->output_offset = 0;
    state->last_dts = INT64_MIN;
    state->last_duration = 0;
}

static int gc_retimestamp_packet_for_segment(
    AVPacket *packet,
    GcMuxStreamState *state,
    uint32_t segment_index
) {
    int64_t original_dts = packet->dts;
    int64_t original_pts = packet->pts;
    int64_t segment_base = original_dts != AV_NOPTS_VALUE ? original_dts : original_pts;
    if (segment_base == AV_NOPTS_VALUE) {
        segment_base = 0;
    }

    if (state->active_segment != segment_index) {
        state->active_segment = segment_index;
        state->segment_first_dts = segment_base;
        state->output_offset = state->last_dts == INT64_MIN
            ? 0
            : state->last_dts + (state->last_duration > 0 ? state->last_duration : 1);
    }

    if (original_dts != AV_NOPTS_VALUE) {
        packet->dts = original_dts - state->segment_first_dts + state->output_offset;
        if (packet->dts <= state->last_dts) {
            packet->dts = state->last_dts + 1;
        }
    }
    if (original_pts != AV_NOPTS_VALUE) {
        packet->pts = original_pts - state->segment_first_dts + state->output_offset;
        if (packet->dts != AV_NOPTS_VALUE && packet->pts < packet->dts) {
            packet->pts = packet->dts;
        }
    }
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
}

static int gc_export_mp4_mux_ranges(
    AVFormatContext *input_format,
    int video_stream,
    const GcPacketRange *ranges,
    uint32_t range_count,
    const char *output_path,
    GcExportResult *result
) {
    GcExportSegment *segments = NULL;
    int ret = gc_build_export_segments(input_format, video_stream, ranges, range_count, &segments);
    if (ret < 0) {
        return ret;
    }

    AVFormatContext *output_format = NULL;
    ret = avformat_alloc_output_context2(&output_format, NULL, "mp4", output_path);
    if (ret < 0 || !output_format) {
        free(segments);
        return ret < 0 ? ret : AVERROR(EINVAL);
    }

    int stream_count = (int)input_format->nb_streams;
    int *stream_map = (int *)malloc(sizeof(int) * (size_t)stream_count);
    GcMuxStreamState *states = (GcMuxStreamState *)malloc(sizeof(GcMuxStreamState) * (size_t)stream_count);
    if (!stream_map || !states) {
        free(stream_map);
        free(states);
        free(segments);
        avformat_free_context(output_format);
        return AVERROR(ENOMEM);
    }
    for (int i = 0; i < stream_count; i++) {
        stream_map[i] = -1;
        gc_mux_state_init(&states[i]);
    }

    for (int i = 0; i < stream_count; i++) {
        AVStream *input_stream = input_format->streams[i];
        if (i != video_stream && input_stream->codecpar->codec_type != AVMEDIA_TYPE_AUDIO) {
            continue;
        }

        AVStream *output_stream = avformat_new_stream(output_format, NULL);
        if (!output_stream) {
            free(stream_map);
            free(states);
            free(segments);
            avformat_free_context(output_format);
            return AVERROR(ENOMEM);
        }

        ret = avcodec_parameters_copy(output_stream->codecpar, input_stream->codecpar);
        if (ret < 0) {
            free(stream_map);
            free(states);
            free(segments);
            avformat_free_context(output_format);
            return ret;
        }
        output_stream->codecpar->codec_tag = 0;
        output_stream->time_base = input_stream->time_base;
        output_stream->avg_frame_rate = input_stream->avg_frame_rate;
        output_stream->r_frame_rate = input_stream->r_frame_rate;
        stream_map[i] = output_stream->index;
    }

    if (!(output_format->oformat->flags & AVFMT_NOFILE)) {
        ret = avio_open(&output_format->pb, output_path, AVIO_FLAG_WRITE);
        if (ret < 0) {
            free(stream_map);
            free(states);
            free(segments);
            avformat_free_context(output_format);
            return ret;
        }
    }

    ret = avformat_write_header(output_format, NULL);
    if (ret < 0) {
        if (!(output_format->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&output_format->pb);
        }
        free(stream_map);
        free(states);
        free(segments);
        avformat_free_context(output_format);
        return ret;
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        ret = AVERROR(ENOMEM);
        goto mux_cleanup;
    }

    int64_t video_packet_index = 0;
    uint32_t video_range_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        int input_index = packet->stream_index;
        if (input_index < 0 || input_index >= stream_count || stream_map[input_index] < 0) {
            if (input_index == video_stream) {
                video_packet_index += 1;
            }
            av_packet_unref(packet);
            continue;
        }

        int segment_index = -1;
        if (input_index == video_stream) {
            if (gc_advance_range(ranges, range_count, &video_range_index, video_packet_index) &&
                video_packet_index >= ranges[video_range_index].start_packet &&
                video_packet_index <= ranges[video_range_index].end_packet) {
                segment_index = (int)video_range_index;
            }
            video_packet_index += 1;
        } else {
            segment_index = gc_find_segment_for_time(
                segments,
                range_count,
                gc_packet_time_us(packet, input_format->streams[input_index])
            );
        }

        if (segment_index < 0) {
            av_packet_unref(packet);
            continue;
        }

        AVStream *input_stream = input_format->streams[input_index];
        AVStream *output_stream = output_format->streams[stream_map[input_index]];
        ret = gc_retimestamp_packet_for_segment(packet, &states[input_index], (uint32_t)segment_index);
        if (ret < 0) {
            av_packet_unref(packet);
            break;
        }

        gc_update_mux_state_after_packet(packet, &states[input_index]);
        packet->stream_index = output_stream->index;
        av_packet_rescale_ts(packet, input_stream->time_base, output_stream->time_base);
        ret = av_interleaved_write_frame(output_format, packet);
        if (ret < 0) {
            break;
        }
        result->packets_written += 1;
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }

mux_cleanup:
    if (packet) {
        av_packet_free(&packet);
    }
    {
        int trailer_ret = av_write_trailer(output_format);
        if (ret >= 0 && trailer_ret < 0) {
            ret = trailer_ret;
        }
    }
    if (!(output_format->oformat->flags & AVFMT_NOFILE)) {
        result->bytes_written = avio_tell(output_format->pb);
        int close_ret = avio_closep(&output_format->pb);
        if (ret >= 0 && close_ret < 0) {
            ret = close_ret;
        }
    }
    free(stream_map);
    free(states);
    free(segments);
    avformat_free_context(output_format);
    return ret;
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

static int gc_retimestamp_packet_cfr_video(AVPacket *packet, GcMuxStreamState *state, int64_t frame_duration) {
    int64_t pts_offset = 0;
    if (packet->pts != AV_NOPTS_VALUE && packet->dts != AV_NOPTS_VALUE) {
        pts_offset = packet->pts - packet->dts;
        if (pts_offset < 0) {
            pts_offset = 0;
        }
    }

    int64_t dts = state->last_dts == INT64_MIN ? 0 : state->last_dts + frame_duration;
    packet->dts = dts;
    packet->pts = dts + pts_offset;
    packet->duration = frame_duration;
    state->last_dts = dts;
    state->last_duration = frame_duration;
    return 0;
}

static int gc_codecpar_compatible(const AVCodecParameters *expected, const AVCodecParameters *actual) {
    if (!expected || !actual) {
        return 0;
    }
    if (expected->codec_type != actual->codec_type ||
        expected->codec_id != actual->codec_id ||
        expected->width != actual->width ||
        expected->height != actual->height ||
        expected->format != actual->format ||
        expected->profile != actual->profile ||
        expected->level != actual->level ||
        expected->extradata_size != actual->extradata_size) {
        return 0;
    }
    if (expected->extradata_size > 0 &&
        memcmp(expected->extradata, actual->extradata, (size_t)expected->extradata_size) != 0) {
        return 0;
    }
    return 1;
}

static AVRational gc_stream_frame_rate(AVStream *stream) {
    if (stream->r_frame_rate.num > 0 && stream->r_frame_rate.den > 0) {
        return stream->r_frame_rate;
    }
    if (stream->avg_frame_rate.num > 0 && stream->avg_frame_rate.den > 0) {
        return stream->avg_frame_rate;
    }
    return (AVRational){0, 1};
}

static int64_t gc_frame_duration_in_time_base(AVRational frame_rate, AVRational time_base) {
    if (frame_rate.num <= 0 || frame_rate.den <= 0) {
        return 1;
    }
    int64_t duration = av_rescale_q(1, (AVRational){frame_rate.den, frame_rate.num}, time_base);
    return duration > 0 ? duration : 1;
}

static int gc_export_raw_video_segment_to_file(
    FILE *output,
    AVFormatContext *input_format,
    int video_stream,
    const GcPacketRange *range,
    enum AVCodecID expected_codec,
    GcExportResult *result
) {
    AVStream *input_stream = input_format->streams[video_stream];
    enum AVCodecID codec_id = input_stream->codecpar->codec_id;
    if (!gc_is_h264_or_hevc(codec_id)) {
        gc_set_export_error(result, "raw export only supports H.264 or HEVC video", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }
    if (expected_codec != AV_CODEC_ID_NONE && codec_id != expected_codec) {
        gc_set_export_error(result, "raw segment video codec differs from the first segment", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    AVBSFContext *bsf = NULL;
    const char *bsf_name = gc_raw_bsf_name(codec_id);
    if (bsf_name) {
        const AVBitStreamFilter *filter = av_bsf_get_by_name(bsf_name);
        if (!filter) {
            return AVERROR_BSF_NOT_FOUND;
        }

        int ret = av_bsf_alloc(filter, &bsf);
        if (ret < 0) {
            return ret;
        }

        ret = avcodec_parameters_copy(bsf->par_in, input_stream->codecpar);
        if (ret < 0) {
            av_bsf_free(&bsf);
            return ret;
        }
        bsf->time_base_in = input_stream->time_base;

        ret = av_bsf_init(bsf);
        if (ret < 0) {
            av_bsf_free(&bsf);
            return ret;
        }
    }

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        av_bsf_free(&bsf);
        return AVERROR(ENOMEM);
    }

    int ret = 0;
    int64_t video_packet_index = 0;
    while ((ret = av_read_frame(input_format, packet)) >= 0) {
        if (packet->stream_index != video_stream) {
            av_packet_unref(packet);
            continue;
        }
        if (video_packet_index > range->end_packet) {
            av_packet_unref(packet);
            ret = 0;
            break;
        }
        if (video_packet_index >= range->start_packet) {
            ret = gc_write_raw_packet(output, bsf, packet, result);
            if (ret < 0) {
                av_packet_unref(packet);
                break;
            }
        } else {
            av_packet_unref(packet);
        }
        video_packet_index += 1;
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }
    if (ret >= 0) {
        ret = gc_flush_raw_bsf(output, bsf, result);
    }

    av_packet_free(&packet);
    av_bsf_free(&bsf);
    return ret;
}

static int gc_export_raw_video_segments(
    const GcExportInputSegment *segments,
    uint32_t segment_count,
    const char *output_path,
    GcExportResult *result
) {
    FILE *output = gc_fopen_utf8_write(output_path);
    if (!output) {
        int ret = gc_file_io_error();
        gc_set_export_error(result, "failed to open raw output file", ret);
        return ret;
    }

    int ret = 0;
    enum AVCodecID expected_codec = AV_CODEC_ID_NONE;
    for (uint32_t i = 0; i < segment_count; i++) {
        AVFormatContext *input_format = NULL;
        int video_stream = -1;
        ret = gc_open_export_input(segments[i].input_path, &input_format, &video_stream);
        if (ret < 0) {
            break;
        }

        enum AVCodecID codec_id = input_format->streams[video_stream]->codecpar->codec_id;
        if (i == 0) {
            expected_codec = codec_id;
        }
        ret = gc_export_raw_video_segment_to_file(
            output,
            input_format,
            video_stream,
            &segments[i].range,
            expected_codec,
            result
        );
        avformat_close_input(&input_format);
        if (ret < 0) {
            break;
        }
    }

    int close_ret = gc_close_file(output);
    if (close_ret < 0 && ret >= 0) {
        gc_set_export_error(result, "failed to close raw output file", close_ret);
        ret = close_ret;
    }
    return ret;
}

int gopcomposer_export_video_range(
    const char *input_path,
    const char *output_path,
    int64_t start_packet,
    int64_t end_packet,
    int32_t output_kind,
    GcExportResult *result
) {
    GcPacketRange range;
    range.start_packet = start_packet;
    range.end_packet = end_packet;
    return gopcomposer_export_video_ranges(input_path, output_path, &range, 1, output_kind, result);
}

int gopcomposer_export_video_ranges(
    const char *input_path,
    const char *output_path,
    const GcPacketRange *ranges,
    uint32_t range_count,
    int32_t output_kind,
    GcExportResult *result
) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!input_path || !output_path || input_path[0] == '\0' || output_path[0] == '\0') {
        gc_set_export_error(result, "input or output path is empty", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }
    int ret = gc_validate_ranges(ranges, range_count);
    if (ret < 0) {
        gc_set_export_error(result, "invalid packet ranges", ret);
        return ret;
    }

    AVFormatContext *input_format = NULL;
    ret = avformat_open_input(&input_format, input_path, NULL, NULL);
    if (ret < 0) {
        gc_set_export_error(result, "avformat_open_input failed", ret);
        return ret;
    }

    ret = avformat_find_stream_info(input_format, NULL);
    if (ret < 0) {
        gc_set_export_error(result, "avformat_find_stream_info failed", ret);
        avformat_close_input(&input_format);
        return ret;
    }

    int video_stream = av_find_best_stream(input_format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        gc_set_export_error(result, "no video stream found", video_stream);
        avformat_close_input(&input_format);
        return video_stream;
    }

    if (output_kind == 0) {
        ret = gc_export_mp4_mux_ranges(input_format, video_stream, ranges, range_count, output_path, result);
    } else if (output_kind == 1) {
        ret = gc_export_raw_video_ranges(input_format, video_stream, ranges, range_count, output_path, result);
    } else {
        ret = AVERROR(EINVAL);
        gc_set_export_error(result, "unknown output kind", ret);
    }

    avformat_close_input(&input_format);

    if (ret < 0 && !result->error) {
        gc_set_export_error(result, "export failed", ret);
    }
    return ret;
}

int gopcomposer_export_video_segments(
    const GcExportInputSegment *segments,
    uint32_t segment_count,
    const char *output_path,
    int32_t output_kind,
    const GcExportOptions *options,
    GcExportResult *result
) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!segments || segment_count == 0 || !output_path || output_path[0] == '\0') {
        gc_set_export_error(result, "export segments or output path is empty", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    for (uint32_t i = 0; i < segment_count; i++) {
        if (!segments[i].input_path || segments[i].input_path[0] == '\0') {
            gc_set_export_error(result, "segment input path is empty", AVERROR(EINVAL));
            return AVERROR(EINVAL);
        }
        if (segments[i].range.start_packet < 0 || segments[i].range.end_packet < segments[i].range.start_packet) {
            gc_set_export_error(result, "invalid segment packet range", AVERROR(EINVAL));
            return AVERROR(EINVAL);
        }
    }

    int ret = 0;
    if (output_kind == 0) {
        ret = gc_export_mp4_segments(segments, segment_count, output_path, options, result);
    } else if (output_kind == 1) {
        ret = gc_export_raw_video_segments(segments, segment_count, output_path, result);
    } else {
        ret = AVERROR(EINVAL);
        gc_set_export_error(result, "unknown output kind", ret);
    }

    if (ret < 0 && !result->error) {
        gc_set_export_error(
            result,
            output_kind == 0
                ? "MP4 segment export failed; source files may have incompatible streams"
                : "raw segment export failed",
            ret
        );
    }
    return ret;
}

void gopcomposer_free_export(GcExportResult *result) {
    if (!result) {
        return;
    }
    free(result->error);
    memset(result, 0, sizeof(*result));
}

