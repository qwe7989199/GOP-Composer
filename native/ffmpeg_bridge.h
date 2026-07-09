#ifndef GOPCOMPOSER_FFMPEG_BRIDGE_H
#define GOPCOMPOSER_FFMPEG_BRIDGE_H

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdio.h>

#include <libavcodec/bsf.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>

typedef struct GcGopInfo {
    int64_t start_frame;
    int64_t end_frame;
    int64_t pts;
    int64_t dts;
    int64_t byte_start;
    int64_t byte_end;
    double start_time;
    double duration;
    int key_flags;
} GcGopInfo;

typedef struct GcStreamInfo {
    int32_t index;
    int32_t media_type;
    int32_t codec_id;
    int32_t width;
    int32_t height;
    int32_t sample_rate;
    int32_t channels;
    int64_t duration_us;
    int64_t bit_rate;
} GcStreamInfo;

typedef struct GcProbeResult {
    int32_t codec_id;
    int32_t width;
    int32_t height;
    int32_t time_base_num;
    int32_t time_base_den;
    int32_t avg_frame_rate_num;
    int32_t avg_frame_rate_den;
    int32_t real_frame_rate_num;
    int32_t real_frame_rate_den;
    int64_t duration_us;
    int64_t frame_count;
    int32_t has_timecode;
    int64_t timecode_start_frame;
    int32_t timecode_rate_num;
    int32_t timecode_rate_den;
    char *timecode;
    uint32_t gop_count;
    GcGopInfo *gops;
    uint32_t stream_count;
    GcStreamInfo *streams;
    char *error;
} GcProbeResult;

typedef struct GcRgbaFrame {
    int32_t width;
    int32_t height;
    int32_t stride;
    uint8_t *pixels;
    char *error;
} GcRgbaFrame;

typedef struct GcRgbaFrameItem {
    int64_t frame_index;
    GcRgbaFrame frame;
} GcRgbaFrameItem;

typedef struct GcRgbaWindow {
    uint32_t frame_count;
    GcRgbaFrameItem *frames;
    char *error;
} GcRgbaWindow;

typedef struct GcDecoderHw {
    AVBufferRef *device_ctx;
    enum AVPixelFormat pix_fmt;
    const char *name;
} GcDecoderHw;

typedef struct GcPreviewSession {
    AVFormatContext *format;
    AVCodecContext *codec;
    GcDecoderHw hw;
    AVPacket *packet;
    AVFrame *frame;
    int video_stream;
    AVRational frame_rate;
    int64_t decoded_index;
    int64_t anchor_frame;
    int64_t anchor_pts;
    int eof;
    AVFrame *prepared_frame;
    int64_t prepared_frame_index;
} GcPreviewSession;
typedef struct GcGlPreview GcGlPreview;

typedef struct GcExportResult {
    int64_t packets_written;
    int64_t bytes_written;
    char *error;
} GcExportResult;

typedef struct GcPacketRange {
    int64_t start_packet;
    int64_t end_packet;
} GcPacketRange;

typedef struct GcExportInputSegment {
    const char *input_path;
    GcPacketRange range;
} GcExportInputSegment;

typedef struct GcExportOptions {
    int32_t include_audio;
    int32_t time_base_num;
    int32_t time_base_den;
    int32_t generate_pts;
    int64_t pts_step;
} GcExportOptions;

typedef void (*GcProbeProgressCallback)(void *user_data, int64_t bytes_read, int64_t total_bytes);

int gopcomposer_probe_gops(const char *path, GcProbeResult *result);
int gopcomposer_probe_video_stream(const char *path, GcProbeResult *result);
int gopcomposer_probe_gops_with_progress(
    const char *path,
    GcProbeResult *result,
    GcProbeProgressCallback callback,
    void *user_data
);
void gopcomposer_free_probe(GcProbeResult *result);
const char *gopcomposer_codec_name(int codec_id);

int gopcomposer_decode_rgba_frame(const char *path, int64_t target_frame, GcRgbaFrame *result);
void gopcomposer_free_rgba_frame(GcRgbaFrame *frame);
int gopcomposer_decode_rgba_window(
    const char *path,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame,
    int32_t frame_count,
    GcRgbaWindow *result
);
void gopcomposer_free_rgba_window(GcRgbaWindow *window);
int gopcomposer_preview_session_open(const char *path, GcPreviewSession **session_out, GcRgbaWindow *error_out);
int gopcomposer_preview_session_decode_window(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t start_frame,
    int32_t frame_count,
    GcRgbaWindow *result
);
const char *gopcomposer_preview_session_accel(GcPreviewSession *session);
void gopcomposer_preview_session_free(GcPreviewSession *session);

int gopcomposer_preview_session_prepare_dx(
    GcPreviewSession *session,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
);
int gopcomposer_gl_preview_create(uint32_t gl_texture, GcGlPreview **preview_out, char **error_out);
void gopcomposer_gl_preview_free(GcGlPreview *preview);
int gopcomposer_preview_session_render_gl(
    GcPreviewSession *session,
    GcGlPreview *preview,
    int64_t anchor_frame,
    int64_t anchor_pts,
    int64_t target_frame,
    char **error_out
);
void gopcomposer_free_string(char *text);

int gopcomposer_export_video_range(
    const char *input_path,
    const char *output_path,
    int64_t start_packet,
    int64_t end_packet,
    int32_t output_kind,
    GcExportResult *result
);
int gopcomposer_export_video_ranges(
    const char *input_path,
    const char *output_path,
    const GcPacketRange *ranges,
    uint32_t range_count,
    int32_t output_kind,
    GcExportResult *result
);
int gopcomposer_export_video_segments(
    const GcExportInputSegment *segments,
    uint32_t segment_count,
    const char *output_path,
    int32_t output_kind,
    const GcExportOptions *options,
    GcExportResult *result
);
void gopcomposer_free_export(GcExportResult *result);

#endif
