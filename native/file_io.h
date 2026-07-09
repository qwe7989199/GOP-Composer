#ifndef GOPCOMPOSER_FILE_IO_H
#define GOPCOMPOSER_FILE_IO_H

#include <stdint.h>
#include <stdio.h>

FILE *gc_fopen_utf8_write(const char *path);
int gc_file_io_error(void);
int gc_write_file(FILE *file, const uint8_t *data, size_t size);
int gc_close_file(FILE *file);

#endif
