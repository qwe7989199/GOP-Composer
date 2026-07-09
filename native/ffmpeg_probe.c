#include "ffmpeg_bridge.h"

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

static void gc_set_error(GcProbeResult *result, const char *prefix, int code) {
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

static int gc_push_gop(GcGopInfo **items, uint32_t *len, uint32_t *cap, GcGopInfo item) {
    if (*len >= *cap) {
        uint32_t next_cap = *cap == 0 ? 256 : (*cap * 2);
        GcGopInfo *next = (GcGopInfo *)realloc(*items, sizeof(GcGopInfo) * next_cap);
        if (!next) {
            return AVERROR(ENOMEM);
        }
        *items = next;
        *cap = next_cap;
    }
    (*items)[*len] = item;
    *len += 1;
    return 0;
}

static int gc_copy_streams(AVFormatContext *format, GcProbeResult *result) {
    if (!format || !result || format->nb_streams == 0) {
        return 0;
    }

    GcStreamInfo *streams = (GcStreamInfo *)calloc(format->nb_streams, sizeof(GcStreamInfo));
    if (!streams) {
        return AVERROR(ENOMEM);
    }

    for (unsigned int i = 0; i < format->nb_streams; i++) {
        AVStream *stream = format->streams[i];
        AVCodecParameters *codecpar = stream ? stream->codecpar : NULL;
        GcStreamInfo *info = &streams[i];
        info->index = (int32_t)i;
        info->media_type = codecpar ? codecpar->codec_type : AVMEDIA_TYPE_UNKNOWN;
        info->codec_id = codecpar ? codecpar->codec_id : AV_CODEC_ID_NONE;
        info->width = codecpar ? codecpar->width : 0;
        info->height = codecpar ? codecpar->height : 0;
        info->sample_rate = codecpar ? codecpar->sample_rate : 0;
        info->channels = codecpar ? codecpar->ch_layout.nb_channels : 0;
        info->duration_us = 0;
        info->bit_rate = codecpar ? codecpar->bit_rate : 0;

        if (stream && stream->duration != AV_NOPTS_VALUE && stream->duration > 0) {
            info->duration_us = av_rescale_q(stream->duration, stream->time_base, AV_TIME_BASE_Q);
        } else if (format->duration > 0) {
            info->duration_us = format->duration;
        }
    }

    result->stream_count = format->nb_streams;
    result->streams = streams;
    return 0;
}

static const char *gc_metadata_value(AVDictionary *metadata, const char *key) {
    AVDictionaryEntry *entry = av_dict_get(metadata, key, NULL, 0);
    return entry && entry->value && entry->value[0] != '\0' ? entry->value : NULL;
}

static const char *gc_find_timecode(AVFormatContext *format, int video_stream) {
    if (!format) {
        return NULL;
    }
    if (video_stream >= 0 && video_stream < (int)format->nb_streams) {
        const char *timecode = gc_metadata_value(format->streams[video_stream]->metadata, "timecode");
        if (timecode) {
            return timecode;
        }
    }

    const char *timecode = gc_metadata_value(format->metadata, "timecode");
    if (timecode) {
        return timecode;
    }

    for (unsigned int i = 0; i < format->nb_streams; i++) {
        AVStream *stream = format->streams[i];
        if (!stream) {
            continue;
        }
        timecode = gc_metadata_value(stream->metadata, "timecode");
        if (timecode) {
            return timecode;
        }
    }
    return NULL;
}

static int gc_timecode_fps(AVStream *stream) {
    AVRational frame_rate = {0, 1};
    if (stream) {
        if (stream->avg_frame_rate.num > 0 && stream->avg_frame_rate.den > 0) {
            frame_rate = stream->avg_frame_rate;
        } else if (stream->r_frame_rate.num > 0 && stream->r_frame_rate.den > 0) {
            frame_rate = stream->r_frame_rate;
        }
    }
    int fps = frame_rate.num > 0 ? (int)llround(av_q2d(frame_rate)) : 0;
    return fps > 0 ? fps : 30;
}

static int gc_parse_timecode_frames(const char *timecode, int fps, int64_t *frames_out) {
    if (!timecode || !frames_out || fps <= 0) {
        return 0;
    }

    int hour = 0;
    int minute = 0;
    int second = 0;
    int frame = 0;
    char separator = 0;
    if (sscanf(timecode, "%d:%d:%d%c%d", &hour, &minute, &second, &separator, &frame) != 5) {
        return 0;
    }
    if (hour < 0 || minute < 0 || minute > 59 || second < 0 || second > 59 || frame < 0 || frame >= fps) {
        return 0;
    }

    int64_t total_minutes = (int64_t)hour * 60 + minute;
    int64_t total_frames = ((int64_t)hour * 3600 + minute * 60 + second) * fps + frame;
    if (separator == ';' && (fps == 30 || fps == 60)) {
        int64_t drop_frames = fps == 60 ? 4 : 2;
        total_frames -= drop_frames * (total_minutes - total_minutes / 10);
    }
    *frames_out = total_frames;
    return 1;
}

static void gc_copy_timecode(AVFormatContext *format, int video_stream, GcProbeResult *result) {
    const char *timecode = gc_find_timecode(format, video_stream);
    if (!timecode || !result) {
        return;
    }

    AVStream *stream = video_stream >= 0 && video_stream < (int)format->nb_streams
        ? format->streams[video_stream]
        : NULL;
    int fps = gc_timecode_fps(stream);
    int64_t start_frame = 0;
    if (!gc_parse_timecode_frames(timecode, fps, &start_frame)) {
        return;
    }

    result->has_timecode = 1;
    result->timecode_start_frame = start_frame;
    result->timecode_rate_num = fps;
    result->timecode_rate_den = 1;
    result->timecode = gc_strdup(timecode);
}

static int64_t gc_packet_progress(AVFormatContext *format, const AVPacket *packet, int64_t total_bytes) {
    int64_t bytes_read = 0;
    if (packet && packet->pos >= 0) {
        bytes_read = packet->pos + (packet->size > 0 ? packet->size : 0);
    } else if (format && format->pb) {
        int64_t tell = avio_tell(format->pb);
        if (tell > 0) {
            bytes_read = tell;
        }
    }
    if (total_bytes > 0 && bytes_read > total_bytes) {
        bytes_read = total_bytes;
    }
    return bytes_read < 0 ? 0 : bytes_read;
}

static void gc_report_probe_progress(
    GcProbeProgressCallback callback,
    void *user_data,
    int64_t bytes_read,
    int64_t total_bytes,
    int64_t *last_reported
) {
    if (!callback) {
        return;
    }

    int should_report = 0;
    if (*last_reported < 0 || bytes_read >= total_bytes) {
        should_report = 1;
    } else if (total_bytes > 0) {
        int64_t step = total_bytes / 10000;
        if (step < 64 * 1024) {
            step = 64 * 1024;
        }
        if (bytes_read - *last_reported >= step) {
            should_report = 1;
        }
    } else if (bytes_read - *last_reported >= 16 * 1024 * 1024) {
        should_report = 1;
    }

    if (should_report) {
        *last_reported = bytes_read;
        callback(user_data, bytes_read, total_bytes);
    }
}

int gopcomposer_probe_gops_with_progress(
    const char *path,
    GcProbeResult *result,
    GcProbeProgressCallback callback,
    void *user_data
) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!path || path[0] == '\0') {
        gc_set_error(result, "input path is empty", 0);
        return AVERROR(EINVAL);
    }

    AVFormatContext *format = NULL;
    int ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        gc_set_error(result, "avformat_open_input failed", ret);
        return ret;
    }

    int64_t total_bytes = format->pb ? avio_size(format->pb) : 0;
    if (total_bytes < 0) {
        total_bytes = 0;
    }
    int64_t last_reported = -1;
    gc_report_probe_progress(callback, user_data, 0, total_bytes, &last_reported);

    ret = avformat_find_stream_info(format, NULL);
    if (ret < 0) {
        gc_set_error(result, "avformat_find_stream_info failed", ret);
        avformat_close_input(&format);
        return ret;
    }
    gc_report_probe_progress(
        callback,
        user_data,
        gc_packet_progress(format, NULL, total_bytes),
        total_bytes,
        &last_reported
    );

    ret = gc_copy_streams(format, result);
    if (ret < 0) {
        gc_set_error(result, "failed to allocate stream list", ret);
        avformat_close_input(&format);
        return ret;
    }

    int video_stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        gc_set_error(result, "no video stream found", video_stream);
        avformat_close_input(&format);
        return video_stream;
    }

    AVStream *stream = format->streams[video_stream];
    AVCodecParameters *codecpar = stream->codecpar;
    AVRational time_base = stream->time_base;

    result->codec_id = codecpar ? codecpar->codec_id : AV_CODEC_ID_NONE;
    result->width = codecpar ? codecpar->width : 0;
    result->height = codecpar ? codecpar->height : 0;
    result->time_base_num = stream->time_base.num;
    result->time_base_den = stream->time_base.den;
    result->avg_frame_rate_num = stream->avg_frame_rate.num;
    result->avg_frame_rate_den = stream->avg_frame_rate.den;
    result->real_frame_rate_num = stream->r_frame_rate.num;
    result->real_frame_rate_den = stream->r_frame_rate.den;
    result->duration_us = format->duration > 0 ? format->duration : 0;
    gc_copy_timecode(format, video_stream, result);

    AVPacket *packet = av_packet_alloc();
    if (!packet) {
        gc_set_error(result, "av_packet_alloc failed", AVERROR(ENOMEM));
        avformat_close_input(&format);
        return AVERROR(ENOMEM);
    }

    GcGopInfo *gops = NULL;
    uint32_t gop_len = 0;
    uint32_t gop_cap = 0;
    int64_t video_packet_index = 0;

    while ((ret = av_read_frame(format, packet)) >= 0) {
        int64_t bytes_read = gc_packet_progress(format, packet, total_bytes);
        gc_report_probe_progress(callback, user_data, bytes_read, total_bytes, &last_reported);

        if (packet->stream_index != video_stream) {
            av_packet_unref(packet);
            continue;
        }

        int is_key = (packet->flags & AV_PKT_FLAG_KEY) != 0;
        if (is_key || gop_len == 0) {
            if (gop_len > 0) {
                GcGopInfo *prev = &gops[gop_len - 1];
                prev->end_frame = video_packet_index - 1;
                if (prev->duration <= 0.0 && packet->pts != AV_NOPTS_VALUE && prev->pts != AV_NOPTS_VALUE) {
                    prev->duration = (packet->pts - prev->pts) * av_q2d(time_base);
                }
            }

            int64_t byte_start = packet->pos >= 0 ? packet->pos : 0;
            int64_t byte_end = byte_start;
            if (packet->pos >= 0 && packet->size > 0) {
                byte_end = packet->pos + packet->size;
            }

            GcGopInfo item;
            memset(&item, 0, sizeof(item));
            item.start_frame = video_packet_index;
            item.end_frame = video_packet_index;
            item.pts = packet->pts;
            item.dts = packet->dts;
            item.byte_start = byte_start;
            item.byte_end = byte_end;
            item.start_time = packet->pts != AV_NOPTS_VALUE ? packet->pts * av_q2d(time_base) : 0.0;
            item.duration = 0.0;
            item.key_flags = packet->flags;

            ret = gc_push_gop(&gops, &gop_len, &gop_cap, item);
            if (ret < 0) {
                gc_set_error(result, "failed to allocate GOP list", ret);
                av_packet_unref(packet);
                break;
            }
        } else if (gop_len > 0 && packet->pos >= 0 && packet->size > 0) {
            GcGopInfo *current = &gops[gop_len - 1];
            int64_t packet_end = packet->pos + packet->size;
            if (packet_end > current->byte_end) {
                current->byte_end = packet_end;
            }
        }

        video_packet_index += 1;
        av_packet_unref(packet);
    }

    if (ret == AVERROR_EOF) {
        ret = 0;
    }

    if (ret >= 0 && gop_len > 0) {
        GcGopInfo *last = &gops[gop_len - 1];
        last->end_frame = video_packet_index > 0 ? video_packet_index - 1 : last->start_frame;
        if (last->duration <= 0.0 && format->duration > 0) {
            double duration = format->duration / 1000000.0;
            if (duration > last->start_time) {
                last->duration = duration - last->start_time;
            }
        }
    }

    av_packet_free(&packet);
    avformat_close_input(&format);

    if (ret < 0) {
        free(gops);
        return ret;
    }

    result->frame_count = video_packet_index;
    result->gop_count = gop_len;
    result->gops = gops;
    if (total_bytes > 0) {
        gc_report_probe_progress(callback, user_data, total_bytes, total_bytes, &last_reported);
    }
    return 0;
}

int gopcomposer_probe_gops(const char *path, GcProbeResult *result) {
    return gopcomposer_probe_gops_with_progress(path, result, NULL, NULL);
}

int gopcomposer_probe_video_stream(const char *path, GcProbeResult *result) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!path || path[0] == '\0') {
        gc_set_error(result, "input path is empty", 0);
        return AVERROR(EINVAL);
    }

    AVFormatContext *format = NULL;
    int ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        gc_set_error(result, "avformat_open_input failed", ret);
        return ret;
    }

    ret = avformat_find_stream_info(format, NULL);
    if (ret < 0) {
        gc_set_error(result, "avformat_find_stream_info failed", ret);
        avformat_close_input(&format);
        return ret;
    }

    ret = gc_copy_streams(format, result);
    if (ret < 0) {
        gc_set_error(result, "failed to allocate stream list", ret);
        avformat_close_input(&format);
        return ret;
    }

    int video_stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        gc_set_error(result, "no video stream found", video_stream);
        avformat_close_input(&format);
        return video_stream;
    }

    AVStream *stream = format->streams[video_stream];
    AVCodecParameters *codecpar = stream->codecpar;
    result->codec_id = codecpar ? codecpar->codec_id : AV_CODEC_ID_NONE;
    result->width = codecpar ? codecpar->width : 0;
    result->height = codecpar ? codecpar->height : 0;
    result->time_base_num = stream->time_base.num;
    result->time_base_den = stream->time_base.den;
    result->avg_frame_rate_num = stream->avg_frame_rate.num;
    result->avg_frame_rate_den = stream->avg_frame_rate.den;
    result->real_frame_rate_num = stream->r_frame_rate.num;
    result->real_frame_rate_den = stream->r_frame_rate.den;
    result->duration_us = format->duration > 0 ? format->duration : 0;
    result->frame_count = stream->nb_frames > 0 ? stream->nb_frames : 0;
    gc_copy_timecode(format, video_stream, result);

    avformat_close_input(&format);
    return 0;
}

void gopcomposer_free_probe(GcProbeResult *result) {
    if (!result) {
        return;
    }
    free(result->gops);
    free(result->streams);
    free(result->timecode);
    free(result->error);
    memset(result, 0, sizeof(*result));
}

const char *gopcomposer_codec_name(int codec_id) {
    const char *name = avcodec_get_name((enum AVCodecID)codec_id);
    return name ? name : "unknown";
}

