/* Local read-only integration. No filesystem IO and no native text conversion. */
#include <stdlib.h>
#include <stdint.h>
#include <limits.h>
#include "readstat.h"
#include "readstat_io_unistd.h"

#ifdef _MSC_VER
#define YAMAA_THREAD_LOCAL __declspec(thread)
#else
#define YAMAA_THREAD_LOCAL _Thread_local
#endif
static YAMAA_THREAD_LOCAL size_t rows_limit, columns_limit, cells_limit, bytes_limit;
static YAMAA_THREAD_LOCAL int limit_hit, number_exact, refusal_resource, refusal_known;
static YAMAA_THREAD_LOCAL int deleted_records_hit;
static YAMAA_THREAD_LOCAL int refused_row, refused_column;
static YAMAA_THREAD_LOCAL size_t refusal_limit, refusal_actual;
static int refuse(int resource, size_t limit, size_t actual, int known) {
    if (!limit_hit) {
        refusal_resource = resource; refusal_limit = limit;
        refusal_actual = actual; refusal_known = known;
    }
    limit_hit = 1;
    return 0;
}
void yamaa_readstat_number(const unsigned char *bytes) {
    uint64_t mantissa = 0;
    for (int i = 1; i < 8; i++) mantissa = (mantissa << 8) | bytes[i];
    if (!mantissa) { number_exact = 1; return; }
    while ((mantissa & 1) == 0) mantissa >>= 1;
    number_exact = mantissa <= UINT64_C(9007199254740991);
}

int yamaa_readstat_admit(size_t rows, size_t columns) {
    if (rows > rows_limit) return refuse(1, rows_limit, rows, 1);
    if (columns > columns_limit) return refuse(2, columns_limit, columns, 1);
    if (columns && rows > cells_limit / columns)
        return refuse(3, cells_limit, rows * columns, rows <= SIZE_MAX / columns);
    return 1;
}
/* Bytes that decompressing SAS records writes count as source bytes. */
int yamaa_readstat_admit_bytes(uint64_t bytes) {
    if (bytes > bytes_limit)
        return refuse(4, bytes_limit, (size_t)bytes, bytes <= SIZE_MAX);
    return 1;
}
/* SAS records deleted in place that disagree with their count; classify the stop. */
readstat_error_t yamaa_readstat_deleted_records(void) {
    deleted_records_hit = 1;
    return READSTAT_ERROR_PARSE;
}
/* A value whose text does not convert, a NUL byte in it; locate the stop by
 * the value's record and field, each from zero. */
readstat_error_t yamaa_readstat_refuse_value(int row, int column, readstat_error_t error) {
    if (error == READSTAT_ERROR_CONVERT_BAD_STRING) {
        refused_row = row; refused_column = column;
    }
    return error;
}
/* The located value of the last parse on this thread, or -1 for each. */
void yamaa_readstat_refused_value(int *row, int *column) {
    *row = refused_row; *column = refused_column;
}
typedef int (*yamaa_metadata)(int64_t rows, int columns, int version,
        const char *encoding, void *user);
typedef int (*yamaa_variable)(int column, const char *name, size_t width,
        int text, void *user);
typedef int (*yamaa_value)(int row, int column, const char *text, double number,
        int missing, int exact, void *user);
typedef struct {
    const unsigned char *bytes;
    size_t length, position;
    yamaa_metadata metadata;
    yamaa_variable variable;
    yamaa_value value;
    void *user;
    int xport;
} yamaa_ctx;



/* Parser construction must never even initialize a filesystem transport. */
readstat_error_t unistd_io_init(readstat_parser_t *parser) {
    (void)parser;
    return READSTAT_OK;
}
static int memory_open(const char *path, void *opaque) {
    (void)path; ((yamaa_ctx *)opaque)->position = 0; return 0;
}
static int memory_close(void *opaque) { (void)opaque; return 0; }
static readstat_off_t memory_seek(readstat_off_t offset,
        readstat_io_flags_t whence, void *opaque) {
    yamaa_ctx *ctx = opaque;
    int64_t base = whence == READSTAT_SEEK_SET ? 0 :
        whence == READSTAT_SEEK_CUR ? (int64_t)ctx->position :
        whence == READSTAT_SEEK_END ? (int64_t)ctx->length : -1;
    if (base < 0 || offset < -base || offset > (int64_t)ctx->length - base)
        return -1;
    ctx->position = (size_t)(base + offset);
    return (readstat_off_t)ctx->position;
}
static ssize_t memory_read(void *buffer, size_t count, void *opaque) {
    yamaa_ctx *ctx = opaque;
    if (count > ctx->length - ctx->position) count = ctx->length - ctx->position;
    memcpy(buffer, ctx->bytes + ctx->position, count);
    ctx->position += count;
    return (ssize_t)count;
}
static readstat_error_t memory_update(long size,
        readstat_progress_handler progress, void *user, void *io) {
    (void)size; (void)progress; (void)user; (void)io; return READSTAT_OK;
}
static int metadata_handler(readstat_metadata_t *metadata, void *opaque) {
    yamaa_ctx *ctx = opaque;
    return ctx->metadata(readstat_get_row_count(metadata),
        readstat_get_var_count(metadata), readstat_get_file_format_version(metadata),
        readstat_get_file_encoding(metadata), ctx->user);
}
static int variable_handler(int index, readstat_variable_t *variable,
        const char *label, void *opaque) {
    (void)label;
    yamaa_ctx *ctx = opaque;
    return ctx->variable(index, readstat_variable_get_name(variable),
        readstat_variable_get_storage_width(variable),
        readstat_variable_get_type(variable) == READSTAT_TYPE_STRING, ctx->user);
}
static int value_handler(int row, readstat_variable_t *variable,
        readstat_value_t value, void *opaque) {
    yamaa_ctx *ctx = opaque;
    return ctx->value(row, readstat_variable_get_index(variable),
        readstat_value_type(value) == READSTAT_TYPE_STRING ? readstat_string_value(value) : NULL,
        readstat_value_type(value) == READSTAT_TYPE_DOUBLE ? readstat_double_value(value) : 0,
        readstat_value_is_missing(value, variable), ctx->xport ? number_exact : 1, ctx->user);
}
int yamaa_readstat_parse(const unsigned char *bytes, size_t length, int xport,
        size_t rows, size_t columns, size_t cells, size_t source_bytes,
        yamaa_metadata metadata, yamaa_variable variable, yamaa_value value,
        void *user, int *resource, size_t *limit, size_t *actual, int *known) {
    yamaa_ctx ctx = { .bytes = bytes, .length = length, .metadata = metadata,
        .variable = variable, .value = value, .user = user, .xport = xport };
    rows_limit = rows; columns_limit = columns; cells_limit = cells;
    bytes_limit = source_bytes;
    limit_hit = 0; deleted_records_hit = 0;
    refused_row = -1; refused_column = -1;
    readstat_parser_t *parser = readstat_parser_init();
    if (!parser) return -6;
    readstat_set_io_ctx(parser, &ctx);
    readstat_set_open_handler(parser, memory_open);
    readstat_set_close_handler(parser, memory_close);
    readstat_set_seek_handler(parser, memory_seek);
    readstat_set_read_handler(parser, memory_read);
    readstat_set_update_handler(parser, memory_update);
    readstat_set_handler_character_encoding(parser, NULL);
    readstat_set_metadata_handler(parser, metadata_handler);
    readstat_set_variable_handler(parser, variable_handler);
    readstat_set_value_handler(parser, value_handler);
    int status = xport ? readstat_parse_xport(parser, "snapshot", &ctx) :
        readstat_parse_sas7bdat(parser, "snapshot", &ctx);
    readstat_parser_free(parser);
    if (limit_hit) {
        *resource = refusal_resource; *limit = refusal_limit;
        *actual = refusal_actual; *known = refusal_known;
        return -2;
    }
    if (deleted_records_hit) return -7;
    if (status == READSTAT_ERROR_MALLOC) return -6;
    if (status == READSTAT_ERROR_UNSUPPORTED_COMPRESSION) return -3;
    if (status == READSTAT_ERROR_CONVERT_BAD_STRING) return -4;
    if (status == READSTAT_ERROR_UNSUPPORTED_FILE_FORMAT_VERSION) return -5;
    return status;
}
