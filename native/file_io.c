#include "file_io.h"

#include <errno.h>
#include <stdlib.h>

#include <libavutil/error.h>

#ifdef _WIN32
#include <windows.h>
#include <wchar.h>
#endif

int gc_file_io_error(void) {
    return errno > 0 ? AVERROR(errno) : AVERROR(EIO);
}

FILE *gc_fopen_utf8_write(const char *path) {
    if (!path || path[0] == '\0') {
        errno = EINVAL;
        return NULL;
    }

#ifdef _WIN32
    int wide_len = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    if (wide_len <= 0) {
        errno = EINVAL;
        return NULL;
    }

    wchar_t *wide_path = (wchar_t *)calloc((size_t)wide_len, sizeof(wchar_t));
    if (!wide_path) {
        errno = ENOMEM;
        return NULL;
    }

    wide_len = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide_path, wide_len);
    if (wide_len <= 0) {
        free(wide_path);
        errno = EINVAL;
        return NULL;
    }

    FILE *file = _wfopen(wide_path, L"wb");
    free(wide_path);
    return file;
#else
    return fopen(path, "wb");
#endif
}

int gc_write_file(FILE *file, const uint8_t *data, size_t size) {
    errno = 0;
    size_t written = fwrite(data, 1, size, file);
    return written == size ? 0 : gc_file_io_error();
}

int gc_close_file(FILE *file) {
    errno = 0;
    return fclose(file) == 0 ? 0 : gc_file_io_error();
}
