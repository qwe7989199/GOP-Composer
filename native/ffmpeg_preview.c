#include "ffmpeg_bridge.h"

#if defined(GOPCOMPOSER_TARGET_WINDOWS)
#include <windows.h>
#endif

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

static void gc_set_rgba_error(GcRgbaFrame *result, const char *prefix, int code) {
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

static void gc_set_window_error(GcRgbaWindow *result, const char *prefix, int code) {
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

static enum AVPixelFormat gc_hw_get_format(AVCodecContext *codec, const enum AVPixelFormat *formats) {
    GcDecoderHw *hw = (GcDecoderHw *)codec->opaque;
    if (!hw) {
        return formats[0];
    }

    for (const enum AVPixelFormat *format = formats; *format != AV_PIX_FMT_NONE; format++) {
        if (*format == hw->pix_fmt) {
            return *format;
        }
    }
    return formats[0];
}

static void gc_configure_decoder_threads(AVCodecContext *codec) {
    codec->thread_count = 1;
    codec->thread_type = FF_THREAD_SLICE;
}

static void gc_release_decoder_hw(GcDecoderHw *hw) {
    if (!hw) {
        return;
    }
    av_buffer_unref(&hw->device_ctx);
    hw->pix_fmt = AV_PIX_FMT_NONE;
    hw->name = NULL;
}

#if defined(GOPCOMPOSER_TARGET_WINDOWS)
static INIT_ONCE gc_d3d11va_once = INIT_ONCE_STATIC_INIT;
static CRITICAL_SECTION gc_d3d11va_lock;
static AVBufferRef *gc_shared_d3d11va_device = NULL;

static BOOL CALLBACK gc_init_d3d11va_lock(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    (void)once;
    (void)parameter;
    (void)context;
    InitializeCriticalSection(&gc_d3d11va_lock);
    return TRUE;
}
#endif

static int gc_create_hwdevice(AVBufferRef **device_ctx, enum AVHWDeviceType device_type) {
#if defined(GOPCOMPOSER_TARGET_WINDOWS)
    if (device_type == AV_HWDEVICE_TYPE_D3D11VA) {
        if (!InitOnceExecuteOnce(&gc_d3d11va_once, gc_init_d3d11va_lock, NULL, NULL)) {
            return AVERROR_EXTERNAL;
        }
        EnterCriticalSection(&gc_d3d11va_lock);
        int ret = 0;
        if (!gc_shared_d3d11va_device) {
            ret = av_hwdevice_ctx_create(&gc_shared_d3d11va_device, device_type, NULL, NULL, 0);
        }
        if (ret >= 0) {
            *device_ctx = av_buffer_ref(gc_shared_d3d11va_device);
            if (!*device_ctx) {
                ret = AVERROR(ENOMEM);
            }
        }
        LeaveCriticalSection(&gc_d3d11va_lock);
        return ret;
    }
#endif
    return av_hwdevice_ctx_create(device_ctx, device_type, NULL, NULL, 0);
}

static int gc_try_configure_hwaccel(
    AVCodecContext *codec,
    const AVCodec *decoder,
    GcDecoderHw *hw,
    const char *name,
    enum AVHWDeviceType device_type
) {
    for (int i = 0;; i++) {
        const AVCodecHWConfig *config = avcodec_get_hw_config(decoder, i);
        if (!config) {
            return AVERROR(ENOENT);
        }
        if (
            (config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX) &&
            config->device_type == device_type
        ) {
            int ret = gc_create_hwdevice(&hw->device_ctx, device_type);
            if (ret < 0) {
                return ret;
            }
            hw->pix_fmt = config->pix_fmt;
            hw->name = name;
            codec->hw_device_ctx = av_buffer_ref(hw->device_ctx);
            if (!codec->hw_device_ctx) {
                gc_release_decoder_hw(hw);
                return AVERROR(ENOMEM);
            }
            codec->get_format = gc_hw_get_format;
            codec->opaque = hw;
            return 0;
        }
    }
}

static void gc_configure_preview_decoder(AVCodecContext *codec, const AVCodec *decoder, GcDecoderHw *hw) {
    gc_configure_decoder_threads(codec);
    hw->pix_fmt = AV_PIX_FMT_NONE;

#if defined(GOPCOMPOSER_TARGET_WINDOWS)
    if (gc_try_configure_hwaccel(codec, decoder, hw, "d3d11va", AV_HWDEVICE_TYPE_D3D11VA) >= 0) {
        return;
    }
    gc_release_decoder_hw(hw);
    (void)gc_try_configure_hwaccel(codec, decoder, hw, "dxva2", AV_HWDEVICE_TYPE_DXVA2);
#elif defined(GOPCOMPOSER_TARGET_MACOS)
    (void)gc_try_configure_hwaccel(codec, decoder, hw, "videotoolbox", AV_HWDEVICE_TYPE_VIDEOTOOLBOX);
#else
    (void)decoder;
#endif
}

static void gc_clear_codec_hw(AVCodecContext *codec, GcDecoderHw *hw) {
    if (codec) {
        av_buffer_unref(&codec->hw_device_ctx);
        codec->get_format = NULL;
        codec->opaque = NULL;
    }
    gc_release_decoder_hw(hw);
}

static int gc_open_decoder_with_fallback(AVCodecContext *codec, const AVCodec *decoder, GcDecoderHw *hw) {
    gc_configure_preview_decoder(codec, decoder, hw);
    int ret = avcodec_open2(codec, decoder, NULL);
    if (ret >= 0 || !hw->device_ctx) {
        return ret;
    }

    gc_clear_codec_hw(codec, hw);
    gc_configure_decoder_threads(codec);
    return avcodec_open2(codec, decoder, NULL);
}

static int gc_store_rgba_from_software_frame(AVFrame *frame, GcRgbaFrame *result) {
    int width = frame->width;
    int height = frame->height;
    if (width <= 0 || height <= 0) {
        return AVERROR(EINVAL);
    }

    struct SwsContext *sws = sws_getContext(
        width,
        height,
        (enum AVPixelFormat)frame->format,
        width,
        height,
        AV_PIX_FMT_RGBA,
        SWS_FAST_BILINEAR,
        NULL,
        NULL,
        NULL
    );
    if (!sws) {
        return AVERROR(EINVAL);
    }

    int stride = width * 4;
    size_t size = (size_t)stride * (size_t)height;
    uint8_t *pixels = (uint8_t *)malloc(size);
    if (!pixels) {
        sws_freeContext(sws);
        return AVERROR(ENOMEM);
    }

    uint8_t *dst_data[4] = { pixels, NULL, NULL, NULL };
    int dst_linesize[4] = { stride, 0, 0, 0 };
    int scaled = sws_scale(sws, (const uint8_t * const *)frame->data, frame->linesize, 0, height, dst_data, dst_linesize);
    sws_freeContext(sws);

    if (scaled != height) {
        free(pixels);
        return AVERROR_EXTERNAL;
    }

    result->width = width;
    result->height = height;
    result->stride = stride;
    result->pixels = pixels;
    return 0;
}

static int gc_store_rgba_frame(AVFrame *frame, GcRgbaFrame *result) {
    if (!frame || !result) {
        return AVERROR(EINVAL);
    }
    if (frame->hw_frames_ctx) {
        AVFrame *software = av_frame_alloc();
        if (!software) {
            return AVERROR(ENOMEM);
        }
        int ret = av_hwframe_transfer_data(software, frame, 0);
        if (ret >= 0) {
            ret = gc_store_rgba_from_software_frame(software, result);
        }
        av_frame_free(&software);
        return ret;
    }
    return gc_store_rgba_from_software_frame(frame, result);
}

static int gc_receive_until_target(
    AVCodecContext *codec,
    AVFrame *frame,
    int64_t target_frame,
    int64_t *decoded_index,
    GcRgbaFrame *result
) {
    while (1) {
        int ret = avcodec_receive_frame(codec, frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return ret;
        }
        if (ret < 0) {
            return ret;
        }

        if (*decoded_index == target_frame) {
            ret = gc_store_rgba_frame(frame, result);
            av_frame_unref(frame);
            return ret >= 0 ? 1 : ret;
        }

        *decoded_index += 1;
        av_frame_unref(frame);
    }
}

static int gc_receive_window(
    AVCodecContext *codec,
    AVFrame *frame,
    AVRational time_base,
    AVRational frame_rate,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame,
    int64_t end_frame,
    int64_t *decoded_index,
    GcRgbaWindow *result
) {
    while (1) {
        int ret = avcodec_receive_frame(codec, frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return ret;
        }
        if (ret < 0) {
            return ret;
        }

        (void)time_base;
        (void)frame_rate;
        (void)anchor_frame;
        (void)anchor_pts;
        int64_t frame_index = *decoded_index;

        if (frame_index >= start_frame && frame_index < end_frame && result->frame_count < (uint32_t)(end_frame - start_frame)) {
            GcRgbaFrameItem *item = &result->frames[result->frame_count];
            item->frame_index = frame_index;
            ret = gc_store_rgba_frame(frame, &item->frame);
            if (ret < 0) {
                av_frame_unref(frame);
                return ret;
            }

            result->frame_count += 1;
            if (frame_index + 1 >= end_frame) {
                *decoded_index += 1;
                av_frame_unref(frame);
                return 1;
            }
        } else if (frame_index >= end_frame && result->frame_count > 0) {
            *decoded_index += 1;
            av_frame_unref(frame);
            return 1;
        }

        *decoded_index += 1;
        av_frame_unref(frame);
    }
}

int gopcomposer_decode_rgba_frame(const char *path, int64_t target_frame, GcRgbaFrame *result) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!path || path[0] == '\0' || target_frame < 0) {
        gc_set_rgba_error(result, "invalid decode request", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    AVFormatContext *format = NULL;
    AVCodecContext *codec = NULL;
    GcDecoderHw hw;
    AVPacket *packet = NULL;
    AVFrame *frame = NULL;
    memset(&hw, 0, sizeof(hw));
    hw.pix_fmt = AV_PIX_FMT_NONE;
    int ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        gc_set_rgba_error(result, "avformat_open_input failed", ret);
        goto cleanup;
    }

    ret = avformat_find_stream_info(format, NULL);
    if (ret < 0) {
        gc_set_rgba_error(result, "avformat_find_stream_info failed", ret);
        goto cleanup;
    }

    int video_stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        ret = video_stream;
        gc_set_rgba_error(result, "no video stream found", ret);
        goto cleanup;
    }

    AVStream *stream = format->streams[video_stream];
    const AVCodec *decoder = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!decoder) {
        ret = AVERROR_DECODER_NOT_FOUND;
        gc_set_rgba_error(result, "decoder not found", ret);
        goto cleanup;
    }

    codec = avcodec_alloc_context3(decoder);
    if (!codec) {
        ret = AVERROR(ENOMEM);
        gc_set_rgba_error(result, "avcodec_alloc_context3 failed", ret);
        goto cleanup;
    }

    ret = avcodec_parameters_to_context(codec, stream->codecpar);
    if (ret < 0) {
        gc_set_rgba_error(result, "avcodec_parameters_to_context failed", ret);
        goto cleanup;
    }

    ret = gc_open_decoder_with_fallback(codec, decoder, &hw);
    if (ret < 0) {
        gc_set_rgba_error(result, "avcodec_open2 failed", ret);
        goto cleanup;
    }

    packet = av_packet_alloc();
    frame = av_frame_alloc();
    if (!packet || !frame) {
        ret = AVERROR(ENOMEM);
        gc_set_rgba_error(result, "frame allocation failed", ret);
        goto cleanup;
    }

    int64_t decoded_index = 0;
    while ((ret = av_read_frame(format, packet)) >= 0) {
        if (packet->stream_index == video_stream) {
            ret = avcodec_send_packet(codec, packet);
            av_packet_unref(packet);
            if (ret < 0) {
                gc_set_rgba_error(result, "avcodec_send_packet failed", ret);
                goto cleanup;
            }

            ret = gc_receive_until_target(codec, frame, target_frame, &decoded_index, result);
            if (ret == 1) {
                ret = 0;
                goto cleanup;
            }
            if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
                gc_set_rgba_error(result, "avcodec_receive_frame failed", ret);
                goto cleanup;
            }
        } else {
            av_packet_unref(packet);
        }
    }

    if (ret == AVERROR_EOF) {
        ret = avcodec_send_packet(codec, NULL);
        if (ret >= 0) {
            ret = gc_receive_until_target(codec, frame, target_frame, &decoded_index, result);
            if (ret == 1) {
                ret = 0;
                goto cleanup;
            }
        }
    }

    if (!result->pixels && ret >= 0) {
        ret = AVERROR(EINVAL);
        gc_set_rgba_error(result, "target frame was not decoded", ret);
    }

cleanup:
    if (ret < 0 && !result->error) {
        gc_set_rgba_error(result, "decode failed", ret);
    }
    av_frame_free(&frame);
    av_packet_free(&packet);
    gc_clear_codec_hw(codec, &hw);
    avcodec_free_context(&codec);
    avformat_close_input(&format);
    return ret;
}

int gopcomposer_decode_rgba_window(
    const char *path,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame,
    int32_t frame_count,
    GcRgbaWindow *result
) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!path || path[0] == '\0' || anchor_frame < 0 || start_frame < 0 || frame_count <= 0) {
        gc_set_window_error(result, "invalid decode window request", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }
    if (frame_count > 240) {
        gc_set_window_error(result, "decode window is too large", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    AVFormatContext *format = NULL;
    AVCodecContext *codec = NULL;
    GcDecoderHw hw;
    AVPacket *packet = NULL;
    AVFrame *frame = NULL;
    memset(&hw, 0, sizeof(hw));
    hw.pix_fmt = AV_PIX_FMT_NONE;
    int ret = avformat_open_input(&format, path, NULL, NULL);
    if (ret < 0) {
        gc_set_window_error(result, "avformat_open_input failed", ret);
        goto cleanup;
    }

    ret = avformat_find_stream_info(format, NULL);
    if (ret < 0) {
        gc_set_window_error(result, "avformat_find_stream_info failed", ret);
        goto cleanup;
    }

    int video_stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (video_stream < 0) {
        ret = video_stream;
        gc_set_window_error(result, "no video stream found", ret);
        goto cleanup;
    }

    AVStream *stream = format->streams[video_stream];
    AVRational frame_rate = stream->avg_frame_rate.num > 0 && stream->avg_frame_rate.den > 0
        ? stream->avg_frame_rate
        : stream->r_frame_rate;
    const AVCodec *decoder = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!decoder) {
        ret = AVERROR_DECODER_NOT_FOUND;
        gc_set_window_error(result, "decoder not found", ret);
        goto cleanup;
    }

    codec = avcodec_alloc_context3(decoder);
    if (!codec) {
        ret = AVERROR(ENOMEM);
        gc_set_window_error(result, "avcodec_alloc_context3 failed", ret);
        goto cleanup;
    }

    ret = avcodec_parameters_to_context(codec, stream->codecpar);
    if (ret < 0) {
        gc_set_window_error(result, "avcodec_parameters_to_context failed", ret);
        goto cleanup;
    }

    ret = gc_open_decoder_with_fallback(codec, decoder, &hw);
    if (ret < 0) {
        gc_set_window_error(result, "avcodec_open2 failed", ret);
        goto cleanup;
    }

    packet = av_packet_alloc();
    frame = av_frame_alloc();
    result->frames = (GcRgbaFrameItem *)calloc((size_t)frame_count, sizeof(GcRgbaFrameItem));
    if (!packet || !frame || !result->frames) {
        ret = AVERROR(ENOMEM);
        gc_set_window_error(result, "decode window allocation failed", ret);
        goto cleanup;
    }

    int64_t decoded_index = 0;
    if (anchor_frame > 0 && anchor_pts != AV_NOPTS_VALUE) {
        int seek_ret = av_seek_frame(format, video_stream, anchor_pts, AVSEEK_FLAG_BACKWARD);
        if (seek_ret >= 0) {
            avcodec_flush_buffers(codec);
            decoded_index = anchor_frame;
        }
    }

    int64_t end_frame = start_frame + frame_count;
    ret = gc_receive_window(
        codec,
        frame,
        stream->time_base,
        frame_rate,
        anchor_frame,
        anchor_pts,
        start_frame,
        end_frame,
        &decoded_index,
        result
    );
    if (ret == 1) {
        ret = 0;
        goto cleanup;
    }
    if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        gc_set_window_error(result, "avcodec_receive_frame failed", ret);
        goto cleanup;
    }

    while ((ret = av_read_frame(format, packet)) >= 0) {
        if (packet->stream_index == video_stream) {
            ret = avcodec_send_packet(codec, packet);
            av_packet_unref(packet);
            if (ret < 0) {
                gc_set_window_error(result, "avcodec_send_packet failed", ret);
                goto cleanup;
            }

            ret = gc_receive_window(
                codec,
                frame,
                stream->time_base,
                frame_rate,
                anchor_frame,
                anchor_pts,
                start_frame,
                end_frame,
                &decoded_index,
                result
            );
            if (ret == 1) {
                ret = 0;
                goto cleanup;
            }
            if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
                gc_set_window_error(result, "avcodec_receive_frame failed", ret);
                goto cleanup;
            }
        } else {
            av_packet_unref(packet);
        }
    }

    if (ret == AVERROR_EOF) {
        ret = avcodec_send_packet(codec, NULL);
        if (ret >= 0) {
            ret = gc_receive_window(
                codec,
                frame,
                stream->time_base,
                frame_rate,
                anchor_frame,
                anchor_pts,
                start_frame,
                end_frame,
                &decoded_index,
                result
            );
            if (ret == 1) {
                ret = 0;
                goto cleanup;
            }
        }
    }

    if (ret >= 0 && result->frame_count == 0) {
        ret = AVERROR(EINVAL);
        gc_set_window_error(result, "decode window produced no frames", ret);
    }

cleanup:
    if (ret < 0 && !result->error) {
        gc_set_window_error(result, "decode window failed", ret);
    }
    av_frame_free(&frame);
    av_packet_free(&packet);
    gc_clear_codec_hw(codec, &hw);
    avcodec_free_context(&codec);
    avformat_close_input(&format);
    return ret;
}

int gopcomposer_preview_session_open(const char *path, GcPreviewSession **session_out, GcRgbaWindow *error_out) {
    if (!session_out) {
        return AVERROR(EINVAL);
    }
    *session_out = NULL;
    if (error_out) {
        memset(error_out, 0, sizeof(*error_out));
    }

    if (!path || path[0] == '\0') {
        if (error_out) {
            gc_set_window_error(error_out, "input path is empty", AVERROR(EINVAL));
        }
        return AVERROR(EINVAL);
    }

    GcPreviewSession *session = (GcPreviewSession *)calloc(1, sizeof(GcPreviewSession));
    if (!session) {
        if (error_out) {
            gc_set_window_error(error_out, "preview session allocation failed", AVERROR(ENOMEM));
        }
        return AVERROR(ENOMEM);
    }
    session->video_stream = -1;
    session->decoded_index = 0;
    session->anchor_frame = 0;
    session->anchor_pts = AV_NOPTS_VALUE;
    session->prepared_frame_index = INT64_MIN;

    int ret = avformat_open_input(&session->format, path, NULL, NULL);
    if (ret < 0) {
        if (error_out) {
            gc_set_window_error(error_out, "avformat_open_input failed", ret);
        }
        goto fail;
    }

    ret = avformat_find_stream_info(session->format, NULL);
    if (ret < 0) {
        if (error_out) {
            gc_set_window_error(error_out, "avformat_find_stream_info failed", ret);
        }
        goto fail;
    }

    session->video_stream = av_find_best_stream(session->format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (session->video_stream < 0) {
        ret = session->video_stream;
        if (error_out) {
            gc_set_window_error(error_out, "no video stream found", ret);
        }
        goto fail;
    }

    AVStream *stream = session->format->streams[session->video_stream];
    session->frame_rate = stream->avg_frame_rate.num > 0 && stream->avg_frame_rate.den > 0
        ? stream->avg_frame_rate
        : stream->r_frame_rate;
    const AVCodec *decoder = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!decoder) {
        ret = AVERROR_DECODER_NOT_FOUND;
        if (error_out) {
            gc_set_window_error(error_out, "decoder not found", ret);
        }
        goto fail;
    }

    session->codec = avcodec_alloc_context3(decoder);
    if (!session->codec) {
        ret = AVERROR(ENOMEM);
        if (error_out) {
            gc_set_window_error(error_out, "avcodec_alloc_context3 failed", ret);
        }
        goto fail;
    }

    ret = avcodec_parameters_to_context(session->codec, stream->codecpar);
    if (ret < 0) {
        if (error_out) {
            gc_set_window_error(error_out, "avcodec_parameters_to_context failed", ret);
        }
        goto fail;
    }

    ret = gc_open_decoder_with_fallback(session->codec, decoder, &session->hw);
    if (ret < 0) {
        if (error_out) {
            gc_set_window_error(error_out, "avcodec_open2 failed", ret);
        }
        goto fail;
    }

    session->packet = av_packet_alloc();
    session->frame = av_frame_alloc();
    session->prepared_frame = av_frame_alloc();
    if (!session->packet || !session->frame || !session->prepared_frame) {
        ret = AVERROR(ENOMEM);
        if (error_out) {
            gc_set_window_error(error_out, "preview session frame allocation failed", ret);
        }
        goto fail;
    }

    *session_out = session;
    return 0;

fail:
    av_frame_free(&session->prepared_frame);
    av_frame_free(&session->frame);
    av_packet_free(&session->packet);
    gc_clear_codec_hw(session->codec, &session->hw);
    avcodec_free_context(&session->codec);
    avformat_close_input(&session->format);
    free(session);
    return ret;
}

static int gc_preview_session_seek(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame
) {
    if (
        anchor_frame == session->anchor_frame &&
        anchor_pts == session->anchor_pts &&
        start_frame >= session->decoded_index &&
        !session->eof
    ) {
        return 0;
    }

    int ret = 0;
    if (anchor_frame > 0 && anchor_pts != AV_NOPTS_VALUE) {
        ret = av_seek_frame(session->format, session->video_stream, anchor_pts, AVSEEK_FLAG_BACKWARD);
        if (ret < 0) {
            return ret;
        }
    } else {
        ret = av_seek_frame(session->format, session->video_stream, 0, AVSEEK_FLAG_BACKWARD);
        if (ret < 0) {
            ret = avformat_seek_file(session->format, session->video_stream, INT64_MIN, 0, INT64_MAX, 0);
            if (ret < 0) {
                return ret;
            }
        }
    }

    avcodec_flush_buffers(session->codec);
    session->decoded_index = anchor_frame;
    session->anchor_frame = anchor_frame;
    session->anchor_pts = anchor_pts;
    session->eof = 0;
    return 0;
}

int gopcomposer_preview_session_decode_window(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame,
    int32_t frame_count,
    GcRgbaWindow *result
) {
    if (!result) {
        return AVERROR(EINVAL);
    }
    memset(result, 0, sizeof(*result));

    if (!session || start_frame < 0 || frame_count <= 0) {
        gc_set_window_error(result, "invalid preview session decode request", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }
    if (frame_count > 240) {
        gc_set_window_error(result, "decode window is too large", AVERROR(EINVAL));
        return AVERROR(EINVAL);
    }

    result->frames = (GcRgbaFrameItem *)calloc((size_t)frame_count, sizeof(GcRgbaFrameItem));
    if (!result->frames) {
        gc_set_window_error(result, "decode window allocation failed", AVERROR(ENOMEM));
        return AVERROR(ENOMEM);
    }

    int ret = gc_preview_session_seek(session, anchor_frame, anchor_pts, start_frame);
    if (ret < 0) {
        gc_set_window_error(result, "preview seek failed", ret);
        return ret;
    }

    int64_t end_frame = start_frame + frame_count;
    AVStream *stream = session->format->streams[session->video_stream];
    ret = gc_receive_window(
        session->codec,
        session->frame,
        stream->time_base,
        session->frame_rate,
        anchor_frame,
        anchor_pts,
        start_frame,
        end_frame,
        &session->decoded_index,
        result
    );
    if (ret == 1) {
        return 0;
    }
    if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        gc_set_window_error(result, "avcodec_receive_frame failed", ret);
        return ret;
    }

    while ((ret = av_read_frame(session->format, session->packet)) >= 0) {
        if (session->packet->stream_index == session->video_stream) {
            ret = avcodec_send_packet(session->codec, session->packet);
            av_packet_unref(session->packet);
            if (ret < 0) {
                gc_set_window_error(result, "avcodec_send_packet failed", ret);
                return ret;
            }

            ret = gc_receive_window(
                session->codec,
                session->frame,
                stream->time_base,
                session->frame_rate,
                anchor_frame,
                anchor_pts,
                start_frame,
                end_frame,
                &session->decoded_index,
                result
            );
            if (ret == 1) {
                return 0;
            }
            if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
                gc_set_window_error(result, "avcodec_receive_frame failed", ret);
                return ret;
            }
        } else {
            av_packet_unref(session->packet);
        }
    }

    if (ret == AVERROR_EOF) {
        session->eof = 1;
        ret = avcodec_send_packet(session->codec, NULL);
        if (ret >= 0) {
            ret = gc_receive_window(
                session->codec,
                session->frame,
                stream->time_base,
                session->frame_rate,
                anchor_frame,
                anchor_pts,
                start_frame,
                end_frame,
                &session->decoded_index,
                result
            );
            if (ret == 1) {
                return 0;
            }
        }
    }

    if (ret >= 0 && result->frame_count == 0) {
        ret = AVERROR(EINVAL);
        gc_set_window_error(result, "decode window produced no frames", ret);
    }
    if (ret < 0 && !result->error) {
        gc_set_window_error(result, "preview session decode failed", ret);
    }
    return ret;
}

const char *gopcomposer_preview_session_accel(GcPreviewSession *session) {
    if (!session || !session->hw.device_ctx || !session->hw.name) {
        return "software";
    }
    return session->hw.name;
}

void gopcomposer_preview_session_free(GcPreviewSession *session) {
    if (!session) {
        return;
    }
    av_frame_free(&session->frame);
    av_frame_free(&session->prepared_frame);
    av_packet_free(&session->packet);
    gc_clear_codec_hw(session->codec, &session->hw);
    avcodec_free_context(&session->codec);
    avformat_close_input(&session->format);
    free(session);
}

void gopcomposer_free_rgba_frame(GcRgbaFrame *frame) {
    if (!frame) {
        return;
    }
    free(frame->pixels);
    free(frame->error);
    memset(frame, 0, sizeof(*frame));
}

void gopcomposer_free_rgba_window(GcRgbaWindow *window) {
    if (!window) {
        return;
    }
    if (window->frames) {
        for (uint32_t i = 0; i < window->frame_count; i++) {
            free(window->frames[i].frame.pixels);
            free(window->frames[i].frame.error);
        }
    }
    free(window->frames);
    free(window->error);
    memset(window, 0, sizeof(*window));
}

void gopcomposer_free_string(char *text) {
    free(text);
}

