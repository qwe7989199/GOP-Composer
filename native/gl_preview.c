#include "ffmpeg_bridge.h"

#if defined(GOPCOMPOSER_TARGET_WINDOWS)

#define COBJMACROS
#include <windows.h>
#include <GL/gl.h>
#include <d3d11.h>
#include <libavutil/hwcontext_d3d11va.h>

#ifndef WGL_ACCESS_READ_ONLY_NV
#define WGL_ACCESS_READ_ONLY_NV 0x0000
#endif

typedef HANDLE(WINAPI *PFNWGLDXOPENDEVICENVPROC)(void *);
typedef BOOL(WINAPI *PFNWGLDXCLOSEDEVICENVPROC)(HANDLE);
typedef HANDLE(WINAPI *PFNWGLDXREGISTEROBJECTNVPROC)(HANDLE, void *, GLuint, GLenum, GLenum);
typedef BOOL(WINAPI *PFNWGLDXUNREGISTEROBJECTNVPROC)(HANDLE, HANDLE);
typedef BOOL(WINAPI *PFNWGLDXLOCKOBJECTSNVPROC)(HANDLE, GLint, HANDLE *);
typedef BOOL(WINAPI *PFNWGLDXUNLOCKOBJECTSNVPROC)(HANDLE, GLint, HANDLE *);

typedef struct GcWglDxInterop {
    PFNWGLDXOPENDEVICENVPROC open_device;
    PFNWGLDXCLOSEDEVICENVPROC close_device;
    PFNWGLDXREGISTEROBJECTNVPROC register_object;
    PFNWGLDXUNREGISTEROBJECTNVPROC unregister_object;
    PFNWGLDXLOCKOBJECTSNVPROC lock_objects;
    PFNWGLDXUNLOCKOBJECTSNVPROC unlock_objects;
} GcWglDxInterop;

struct GcGlPreview {
    GLuint gl_texture;
    ID3D11Device *device;
    ID3D11DeviceContext *context;
    ID3D11VideoDevice *video_device;
    ID3D11VideoContext *video_context;
    ID3D11VideoProcessorEnumerator *enumerator;
    ID3D11VideoProcessor *processor;
    ID3D11Query *copy_done_query;
    ID3D11Texture2D *texture;
    ID3D11RenderTargetView *rtv;
    ID3D11VideoProcessorOutputView *output_view;
    HANDLE interop_device;
    HANDLE interop_object;
    UINT source_width;
    UINT source_height;
    UINT output_width;
    UINT output_height;
    int locked;
};

static GcWglDxInterop gc_wgl;

static void gc_gl_release_unknown(void **object) {
    if (object && *object) {
        IUnknown_Release((IUnknown *)(*object));
        *object = NULL;
    }
}

static char *gc_gl_strdup(const char *text) {
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

static void gc_gl_set_error(char **error_out, const char *message) {
    if (error_out) {
        free(*error_out);
        *error_out = gc_gl_strdup(message);
    }
}

static int gc_gl_hresult_failed(HRESULT hr) {
    return FAILED(hr) ? AVERROR_EXTERNAL : 0;
}

static int gc_gl_load_wgl(char **error_out) {
    if (gc_wgl.open_device) {
        return 0;
    }
    gc_wgl.open_device = (PFNWGLDXOPENDEVICENVPROC)wglGetProcAddress("wglDXOpenDeviceNV");
    gc_wgl.close_device = (PFNWGLDXCLOSEDEVICENVPROC)wglGetProcAddress("wglDXCloseDeviceNV");
    gc_wgl.register_object = (PFNWGLDXREGISTEROBJECTNVPROC)wglGetProcAddress("wglDXRegisterObjectNV");
    gc_wgl.unregister_object = (PFNWGLDXUNREGISTEROBJECTNVPROC)wglGetProcAddress("wglDXUnregisterObjectNV");
    gc_wgl.lock_objects = (PFNWGLDXLOCKOBJECTSNVPROC)wglGetProcAddress("wglDXLockObjectsNV");
    gc_wgl.unlock_objects = (PFNWGLDXUNLOCKOBJECTSNVPROC)wglGetProcAddress("wglDXUnlockObjectsNV");
    if (
        !gc_wgl.open_device ||
        !gc_wgl.close_device ||
        !gc_wgl.register_object ||
        !gc_wgl.unregister_object ||
        !gc_wgl.lock_objects ||
        !gc_wgl.unlock_objects
    ) {
        gc_gl_set_error(error_out, "WGL_NV_DX_interop is not available");
        return AVERROR(ENOSYS);
    }
    return 0;
}

int gopcomposer_gl_preview_create(uint32_t gl_texture, GcGlPreview **preview_out, char **error_out) {
    if (error_out) {
        *error_out = NULL;
    }
    if (!preview_out || gl_texture == 0) {
        gc_gl_set_error(error_out, "invalid GL preview create request");
        return AVERROR(EINVAL);
    }
    int ret = gc_gl_load_wgl(error_out);
    if (ret < 0) {
        return ret;
    }
    GcGlPreview *preview = (GcGlPreview *)calloc(1, sizeof(GcGlPreview));
    if (!preview) {
        gc_gl_set_error(error_out, "GL preview allocation failed");
        return AVERROR(ENOMEM);
    }
    preview->gl_texture = (GLuint)gl_texture;
    *preview_out = preview;
    return 0;
}

static void gc_gl_release_target(GcGlPreview *preview) {
    if (!preview) {
        return;
    }
    if (preview->interop_object) {
        if (preview->locked) {
            (void)gc_wgl.unlock_objects(preview->interop_device, 1, &preview->interop_object);
            preview->locked = 0;
        }
        (void)gc_wgl.unregister_object(preview->interop_device, preview->interop_object);
        preview->interop_object = NULL;
    }
    gc_gl_release_unknown((void **)&preview->output_view);
    gc_gl_release_unknown((void **)&preview->rtv);
    gc_gl_release_unknown((void **)&preview->texture);
    gc_gl_release_unknown((void **)&preview->processor);
    gc_gl_release_unknown((void **)&preview->enumerator);
    preview->source_width = 0;
    preview->source_height = 0;
    preview->output_width = 0;
    preview->output_height = 0;
}

static void gc_gl_release_device(GcGlPreview *preview) {
    if (!preview) {
        return;
    }
    gc_gl_release_target(preview);
    gc_gl_release_unknown((void **)&preview->copy_done_query);
    if (preview->interop_device) {
        (void)gc_wgl.close_device(preview->interop_device);
        preview->interop_device = NULL;
    }
    gc_gl_release_unknown((void **)&preview->video_context);
    gc_gl_release_unknown((void **)&preview->video_device);
    gc_gl_release_unknown((void **)&preview->context);
    gc_gl_release_unknown((void **)&preview->device);
}

void gopcomposer_gl_preview_free(GcGlPreview *preview) {
    if (!preview) {
        return;
    }
    gc_gl_release_device(preview);
    free(preview);
}

static int gc_gl_ensure_device(GcPreviewSession *session, GcGlPreview *preview, char **error_out) {
    if (!session->hw.device_ctx || !session->hw.name || strcmp(session->hw.name, "d3d11va") != 0) {
        gc_gl_set_error(error_out, "GL interop preview requires FFmpeg d3d11va frames");
        return AVERROR(ENOSYS);
    }
    AVHWDeviceContext *device_ctx = (AVHWDeviceContext *)session->hw.device_ctx->data;
    AVD3D11VADeviceContext *d3d11 = (AVD3D11VADeviceContext *)device_ctx->hwctx;
    if (!d3d11 || !d3d11->device || !d3d11->device_context) {
        gc_gl_set_error(error_out, "FFmpeg d3d11va device is not available");
        return AVERROR(EINVAL);
    }
    if (preview->device && preview->device == d3d11->device) {
        return 0;
    }
    gc_gl_release_device(preview);
    preview->device = d3d11->device;
    preview->context = d3d11->device_context;
    ID3D11Device_AddRef(preview->device);
    ID3D11DeviceContext_AddRef(preview->context);

    HRESULT hr = ID3D11Device_QueryInterface(preview->device, &IID_ID3D11VideoDevice, (void **)&preview->video_device);
    if (SUCCEEDED(hr)) {
        hr = ID3D11DeviceContext_QueryInterface(preview->context, &IID_ID3D11VideoContext, (void **)&preview->video_context);
    }
    if (FAILED(hr)) {
        gc_gl_set_error(error_out, "D3D11 video processor interfaces are not available");
        return gc_gl_hresult_failed(hr);
    }
    preview->interop_device = gc_wgl.open_device(preview->device);
    if (!preview->interop_device) {
        gc_gl_set_error(error_out, "wglDXOpenDeviceNV failed");
        return AVERROR_EXTERNAL;
    }
    return 0;
}

static int gc_gl_ensure_processor(
    GcGlPreview *preview,
    UINT source_width,
    UINT source_height,
    UINT output_width,
    UINT output_height,
    char **error_out
) {
    if (
        preview->enumerator &&
        preview->processor &&
        preview->source_width == source_width &&
        preview->source_height == source_height &&
        preview->output_width == output_width &&
        preview->output_height == output_height
    ) {
        return 0;
    }
    gc_gl_release_unknown((void **)&preview->processor);
    gc_gl_release_unknown((void **)&preview->enumerator);

    D3D11_VIDEO_PROCESSOR_CONTENT_DESC desc;
    memset(&desc, 0, sizeof(desc));
    desc.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
    desc.InputWidth = source_width;
    desc.InputHeight = source_height;
    desc.OutputWidth = output_width;
    desc.OutputHeight = output_height;
    desc.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;

    HRESULT hr = ID3D11VideoDevice_CreateVideoProcessorEnumerator(preview->video_device, &desc, &preview->enumerator);
    if (SUCCEEDED(hr)) {
        hr = ID3D11VideoDevice_CreateVideoProcessor(preview->video_device, preview->enumerator, 0, &preview->processor);
    }
    if (FAILED(hr)) {
        gc_gl_set_error(error_out, "failed to create D3D11 video processor");
        return gc_gl_hresult_failed(hr);
    }
    preview->source_width = source_width;
    preview->source_height = source_height;
    preview->output_width = output_width;
    preview->output_height = output_height;
    return 0;
}

static int gc_gl_ensure_target(GcGlPreview *preview, UINT width, UINT height, char **error_out) {
    if (preview->texture && preview->output_width == width && preview->output_height == height && preview->interop_object) {
        return 0;
    }
    gc_gl_release_target(preview);

    D3D11_TEXTURE2D_DESC desc;
    memset(&desc, 0, sizeof(desc));
    desc.Width = width;
    desc.Height = height;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.Usage = D3D11_USAGE_DEFAULT;
    desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;

    HRESULT hr = ID3D11Device_CreateTexture2D(preview->device, &desc, NULL, &preview->texture);
    if (SUCCEEDED(hr)) {
        hr = ID3D11Device_CreateRenderTargetView(preview->device, (ID3D11Resource *)preview->texture, NULL, &preview->rtv);
    }
    if (FAILED(hr)) {
        gc_gl_set_error(error_out, "failed to create D3D11 GL interop texture");
        return gc_gl_hresult_failed(hr);
    }
    preview->interop_object = gc_wgl.register_object(
        preview->interop_device,
        preview->texture,
        preview->gl_texture,
        GL_TEXTURE_2D,
        WGL_ACCESS_READ_ONLY_NV
    );
    if (!preview->interop_object) {
        gc_gl_set_error(error_out, "wglDXRegisterObjectNV failed");
        return AVERROR_EXTERNAL;
    }
    preview->output_width = width;
    preview->output_height = height;
    return 0;
}

static int gc_gl_wait_for_copy(GcGlPreview *preview, char **error_out) {
    if (!preview->copy_done_query) {
        D3D11_QUERY_DESC desc;
        memset(&desc, 0, sizeof(desc));
        desc.Query = D3D11_QUERY_EVENT;
        HRESULT hr = ID3D11Device_CreateQuery(preview->device, &desc, &preview->copy_done_query);
        if (FAILED(hr)) {
            gc_gl_set_error(error_out, "failed to create D3D11 copy sync query");
            return gc_gl_hresult_failed(hr);
        }
    }
    ID3D11DeviceContext_End(preview->context, (ID3D11Asynchronous *)preview->copy_done_query);
    ID3D11DeviceContext_Flush(preview->context);
    for (int attempt = 0; attempt < 1000; attempt++) {
        HRESULT hr = ID3D11DeviceContext_GetData(
            preview->context,
            (ID3D11Asynchronous *)preview->copy_done_query,
            NULL,
            0,
            0
        );
        if (hr == S_OK) {
            return 0;
        }
        if (hr != S_FALSE) {
            gc_gl_set_error(error_out, "D3D11 copy sync query failed");
            return gc_gl_hresult_failed(hr);
        }
        Sleep(0);
    }
    return 0;
}

static int gc_gl_present_frame(GcPreviewSession *session, GcGlPreview *preview, AVFrame *frame, char **error_out) {
    if (!frame || frame->format != AV_PIX_FMT_D3D11 || !frame->data[0]) {
        gc_gl_set_error(error_out, "decoded frame is not a D3D11 hardware surface");
        return AVERROR(ENOSYS);
    }
    int ret = gc_gl_ensure_device(session, preview, error_out);
    if (ret < 0) {
        return ret;
    }
    ret = gc_gl_ensure_target(preview, (UINT)frame->width, (UINT)frame->height, error_out);
    if (ret < 0) {
        return ret;
    }
    ret = gc_gl_ensure_processor(preview, (UINT)frame->width, (UINT)frame->height, preview->output_width, preview->output_height, error_out);
    if (ret < 0) {
        return ret;
    }
    if (preview->locked) {
        if (!gc_wgl.unlock_objects(preview->interop_device, 1, &preview->interop_object)) {
            gc_gl_set_error(error_out, "wglDXUnlockObjectsNV failed");
            return AVERROR_EXTERNAL;
        }
        preview->locked = 0;
    }

    ID3D11Texture2D *source = (ID3D11Texture2D *)frame->data[0];
    UINT array_slice = (UINT)(uintptr_t)frame->data[1];
    ID3D11VideoProcessorInputView *input_view = NULL;
    if (!preview->output_view) {
        D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC output_desc;
        memset(&output_desc, 0, sizeof(output_desc));
        output_desc.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
        HRESULT hr = ID3D11VideoDevice_CreateVideoProcessorOutputView(
            preview->video_device,
            (ID3D11Resource *)preview->texture,
            preview->enumerator,
            &output_desc,
            &preview->output_view
        );
        if (FAILED(hr)) {
            gc_gl_set_error(error_out, "failed to create D3D11 GL output view");
            return gc_gl_hresult_failed(hr);
        }
    }

    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC input_desc;
    memset(&input_desc, 0, sizeof(input_desc));
    input_desc.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
    input_desc.Texture2D.ArraySlice = array_slice;
    HRESULT hr = ID3D11VideoDevice_CreateVideoProcessorInputView(
        preview->video_device,
        (ID3D11Resource *)source,
        preview->enumerator,
        &input_desc,
        &input_view
    );
    if (SUCCEEDED(hr)) {
        RECT rect = {0, 0, frame->width, frame->height};
        ID3D11VideoContext_VideoProcessorSetStreamFrameFormat(
            preview->video_context,
            preview->processor,
            0,
            D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE
        );
        ID3D11VideoContext_VideoProcessorSetStreamAutoProcessingMode(
            preview->video_context,
            preview->processor,
            0,
            FALSE
        );
        ID3D11VideoContext_VideoProcessorSetStreamAlpha(
            preview->video_context,
            preview->processor,
            0,
            TRUE,
            1.0f
        );
        ID3D11VideoContext_VideoProcessorSetOutputAlphaFillMode(
            preview->video_context,
            preview->processor,
            D3D11_VIDEO_PROCESSOR_ALPHA_FILL_MODE_OPAQUE,
            0
        );
        ID3D11VideoContext_VideoProcessorSetStreamSourceRect(preview->video_context, preview->processor, 0, TRUE, &rect);
        ID3D11VideoContext_VideoProcessorSetStreamDestRect(preview->video_context, preview->processor, 0, TRUE, &rect);
        ID3D11VideoContext_VideoProcessorSetOutputTargetRect(preview->video_context, preview->processor, TRUE, &rect);
        D3D11_VIDEO_PROCESSOR_STREAM stream;
        memset(&stream, 0, sizeof(stream));
        stream.Enable = TRUE;
        stream.pInputSurface = input_view;
        hr = ID3D11VideoContext_VideoProcessorBlt(preview->video_context, preview->processor, preview->output_view, 0, 1, &stream);
    }
    gc_gl_release_unknown((void **)&input_view);
    if (FAILED(hr)) {
        gc_gl_set_error(error_out, "failed to copy D3D11 frame into GL interop texture");
        return gc_gl_hresult_failed(hr);
    }
    ret = gc_gl_wait_for_copy(preview, error_out);
    if (ret < 0) {
        return ret;
    }
    if (!gc_wgl.lock_objects(preview->interop_device, 1, &preview->interop_object)) {
        gc_gl_set_error(error_out, "wglDXLockObjectsNV failed");
        return AVERROR_EXTERNAL;
    }
    preview->locked = 1;
    return 0;
}

static int64_t gc_gl_frame_index(GcPreviewSession *session, AVFrame *frame, int64_t anchor_frame, int64_t anchor_pts) {
    (void)frame;
    (void)anchor_frame;
    (void)anchor_pts;
    return session->decoded_index;
}

static void gc_gl_clear_prepared(GcPreviewSession *session) {
    if (session && session->prepared_frame) {
        av_frame_unref(session->prepared_frame);
    }
    if (session) {
        session->prepared_frame_index = INT64_MIN;
    }
}

static int gc_gl_prepare_frame(GcPreviewSession *session, AVFrame *frame, int64_t target_frame, char **error_out) {
    if (!session->prepared_frame) {
        gc_gl_set_error(error_out, "GL preview prepare frame is not allocated");
        return AVERROR(ENOMEM);
    }
    if (!frame || frame->format != AV_PIX_FMT_D3D11 || !frame->data[0]) {
        gc_gl_set_error(error_out, "prepared frame is not a D3D11 hardware surface");
        return AVERROR(ENOSYS);
    }
    av_frame_unref(session->prepared_frame);
    int ret = av_frame_ref(session->prepared_frame, frame);
    if (ret < 0) {
        gc_gl_set_error(error_out, "failed to retain prepared D3D11 frame");
        session->prepared_frame_index = INT64_MIN;
        return ret;
    }
    session->prepared_frame_index = target_frame;
    return 0;
}

static int gc_gl_seek(GcPreviewSession *session, int64_t anchor_frame, int64_t anchor_pts, int64_t target_frame) {
    if (
        anchor_frame == session->anchor_frame &&
        anchor_pts == session->anchor_pts &&
        target_frame >= session->decoded_index &&
        !session->eof
    ) {
        return 0;
    }
    gc_gl_clear_prepared(session);
    int ret = anchor_frame > 0 && anchor_pts != AV_NOPTS_VALUE
        ? av_seek_frame(session->format, session->video_stream, anchor_pts, AVSEEK_FLAG_BACKWARD)
        : av_seek_frame(session->format, session->video_stream, 0, AVSEEK_FLAG_BACKWARD);
    if (ret < 0 && anchor_frame <= 0) {
        ret = avformat_seek_file(session->format, session->video_stream, INT64_MIN, 0, INT64_MAX, 0);
    }
    if (ret < 0) {
        return ret;
    }
    avcodec_flush_buffers(session->codec);
    session->decoded_index = anchor_frame;
    session->anchor_frame = anchor_frame;
    session->anchor_pts = anchor_pts;
    session->eof = 0;
    return 0;
}

static int gc_gl_receive_prepare(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    while (1) {
        int ret = avcodec_receive_frame(session->codec, session->frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return ret;
        }
        if (ret < 0) {
            gc_gl_set_error(error_out, "avcodec_receive_frame failed");
            return ret;
        }
        int64_t frame_index = gc_gl_frame_index(session, session->frame, anchor_frame, anchor_pts);
        if (frame_index == target_frame) {
            ret = gc_gl_prepare_frame(session, session->frame, target_frame, error_out);
            session->decoded_index += 1;
            av_frame_unref(session->frame);
            return ret >= 0 ? 1 : ret;
        }
        session->decoded_index += 1;
        av_frame_unref(session->frame);
        if (frame_index > target_frame) {
            return AVERROR(EAGAIN);
        }
    }
}

static int gc_gl_receive_target(
    GcPreviewSession *session,
    GcGlPreview *preview,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    while (1) {
        int ret = avcodec_receive_frame(session->codec, session->frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return ret;
        }
        if (ret < 0) {
            gc_gl_set_error(error_out, "avcodec_receive_frame failed");
            return ret;
        }
        int64_t frame_index = gc_gl_frame_index(session, session->frame, anchor_frame, anchor_pts);
        if (frame_index == target_frame) {
            ret = gc_gl_present_frame(session, preview, session->frame, error_out);
            session->decoded_index += 1;
            av_frame_unref(session->frame);
            return ret >= 0 ? 1 : ret;
        }
        session->decoded_index += 1;
        av_frame_unref(session->frame);
        if (frame_index > target_frame) {
            return AVERROR(EAGAIN);
        }
    }
}

int gopcomposer_preview_session_render_gl(
    GcPreviewSession *session,
    GcGlPreview *preview,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    if (error_out) {
        *error_out = NULL;
    }
    if (!session || !preview || target_frame < 0) {
        gc_gl_set_error(error_out, "invalid GL preview render request");
        return AVERROR(EINVAL);
    }
    if (session->prepared_frame && session->prepared_frame_index == target_frame) {
        int ret = gc_gl_present_frame(session, preview, session->prepared_frame, error_out);
        gc_gl_clear_prepared(session);
        return ret;
    }

    int ret = gc_gl_seek(session, anchor_frame, anchor_pts, target_frame);
    if (ret < 0) {
        gc_gl_set_error(error_out, "GL preview seek failed");
        return ret;
    }
    ret = gc_gl_receive_target(session, preview, anchor_frame, anchor_pts, target_frame, error_out);
    if (ret == 1) {
        return 0;
    }
    if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        return ret;
    }
    while ((ret = av_read_frame(session->format, session->packet)) >= 0) {
        if (session->packet->stream_index == session->video_stream) {
            ret = avcodec_send_packet(session->codec, session->packet);
            av_packet_unref(session->packet);
            if (ret < 0) {
                gc_gl_set_error(error_out, "avcodec_send_packet failed");
                return ret;
            }
            ret = gc_gl_receive_target(session, preview, anchor_frame, anchor_pts, target_frame, error_out);
            if (ret == 1) {
                return 0;
            }
            if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
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
            ret = gc_gl_receive_target(session, preview, anchor_frame, anchor_pts, target_frame, error_out);
            if (ret == 1) {
                return 0;
            }
        }
    }
    gc_gl_set_error(error_out, "target frame was not rendered by GL preview");
    return ret < 0 ? ret : AVERROR(EINVAL);
}

int gopcomposer_preview_session_prepare_dx(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    if (error_out) {
        *error_out = NULL;
    }
    if (!session || target_frame < 0) {
        gc_gl_set_error(error_out, "invalid GL preview prepare request");
        return AVERROR(EINVAL);
    }
    if (session->prepared_frame && session->prepared_frame_index == target_frame) {
        return 0;
    }

    int ret = gc_gl_seek(session, anchor_frame, anchor_pts, target_frame);
    if (ret < 0) {
        gc_gl_set_error(error_out, "GL preview prepare seek failed");
        return ret;
    }
    ret = gc_gl_receive_prepare(session, anchor_frame, anchor_pts, target_frame, error_out);
    if (ret == 1) {
        return 0;
    }
    if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
        return ret;
    }
    while ((ret = av_read_frame(session->format, session->packet)) >= 0) {
        if (session->packet->stream_index == session->video_stream) {
            ret = avcodec_send_packet(session->codec, session->packet);
            av_packet_unref(session->packet);
            if (ret < 0) {
                gc_gl_set_error(error_out, "avcodec_send_packet failed");
                return ret;
            }
            ret = gc_gl_receive_prepare(session, anchor_frame, anchor_pts, target_frame, error_out);
            if (ret == 1) {
                return 0;
            }
            if (ret < 0 && ret != AVERROR(EAGAIN) && ret != AVERROR_EOF) {
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
            ret = gc_gl_receive_prepare(session, anchor_frame, anchor_pts, target_frame, error_out);
            if (ret == 1) {
                return 0;
            }
        }
    }
    gc_gl_set_error(error_out, "target frame was not prepared by GL preview");
    return ret < 0 ? ret : AVERROR(EINVAL);
}

#else

struct GcGlPreview {
    int unused;
};

int gopcomposer_gl_preview_create(uint32_t gl_texture, GcGlPreview **preview_out, char **error_out) {
    (void)gl_texture;
    if (preview_out) {
        *preview_out = NULL;
    }
    if (error_out) {
        *error_out = NULL;
    }
    return AVERROR(ENOSYS);
}

void gopcomposer_gl_preview_free(GcGlPreview *preview) {
    (void)preview;
}

int gopcomposer_preview_session_render_gl(
    GcPreviewSession *session,
    GcGlPreview *preview,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    (void)session;
    (void)preview;
    (void)anchor_frame;
    (void)anchor_pts;
    (void)target_frame;
    if (error_out) {
        *error_out = NULL;
    }
    return AVERROR(ENOSYS);
}

int gopcomposer_preview_session_prepare_dx(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
) {
    (void)session;
    (void)anchor_frame;
    (void)anchor_pts;
    (void)target_frame;
    if (error_out) {
        *error_out = NULL;
    }
    return AVERROR(ENOSYS);
}

#endif
