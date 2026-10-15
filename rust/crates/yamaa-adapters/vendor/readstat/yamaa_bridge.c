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
static YAMAA_THREAD_LOCAL size_t work_limit, rows_limit, columns_limit, cells_limit;
static YAMAA_THREAD_LOCAL size_t work_remaining;
static YAMAA_THREAD_LOCAL int limit_hit, number_exact, refusal_resource, refusal_known;
static YAMAA_THREAD_LOCAL size_t refusal_limit, refusal_actual;
static int refuse(int resource, size_t limit, size_t actual, int known) {
    if (!limit_hit) {
        refusal_resource = resource; refusal_limit = limit;
        refusal_actual = actual; refusal_known = known;
    }
    limit_hit = 1;
    return 0;
}
/* Independent stages reuse the work allowance. Record their largest admitted
 * charge, rather than claiming an allocator total or a CPU measurement. */
static void charge_work(size_t bytes) {
    if (bytes > work_limit - work_remaining) work_remaining = work_limit - bytes;
}
void yamaa_readstat_number(const unsigned char *bytes) {
    uint64_t mantissa = 0;
    for (int i = 1; i < 8; i++) mantissa = (mantissa << 8) | bytes[i];
    if (!mantissa) { number_exact = 1; return; }
    while ((mantissa & 1) == 0) mantissa >>= 1;
    number_exact = mantissa <= UINT64_C(9007199254740991);
}

int yamaa_readstat_admit(size_t rows, size_t columns, size_t bytes) {
    if (rows > rows_limit) return refuse(1, rows_limit, rows, 1);
    if (columns > columns_limit) return refuse(2, columns_limit, columns, 1);
    if (bytes > work_limit) return refuse(4, work_limit, bytes, 1);
    if (columns && rows > cells_limit / columns)
        return refuse(3, cells_limit, rows * columns, rows <= SIZE_MAX / columns);
    charge_work(bytes);
    return 1;
}
int yamaa_readstat_allocation(size_t bytes, int known) {
    if (!known) return refuse(7, work_limit, bytes, 0);
    return yamaa_readstat_admit(0, 0, bytes);
}

static size_t saturated_add(size_t a, size_t b) {
    return b > SIZE_MAX - a ? SIZE_MAX : a + b;
}
static size_t saturated_mul(size_t a, size_t b) {
    return b && a > SIZE_MAX / b ? SIZE_MAX : a * b;
}

typedef int (*yamaa_metadata)(int64_t rows, int columns, int version,
        const char *encoding, void *user);
typedef int (*yamaa_variable)(int column, const char *name, size_t width,
        int text, void *user);
typedef int (*yamaa_value)(int row, int column, const char *text, double number,
        int missing, int exact, void *user);
typedef struct {
    const unsigned char *bytes;
    size_t length, position, reads, operations, read_limit, operation_limit;
    yamaa_metadata metadata;
    yamaa_variable variable;
    yamaa_value value;
    void *user;
    int xport, measured;
} yamaa_ctx;

static int admit_operation(yamaa_ctx *ctx) {
    int known = ctx->operations < SIZE_MAX;
    size_t next = ctx->operations + 1;
    if (!known || next > ctx->operation_limit)
        return refuse(5, ctx->operation_limit, next, known);
    ctx->operations = next;
    return 1;
}

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
    if (!admit_operation(ctx)) return -1;
    if (base < 0 || offset < -base || offset > (int64_t)ctx->length - base)
        return -1;
    ctx->position = (size_t)(base + offset);
    return (readstat_off_t)ctx->position;
}
static ssize_t memory_read(void *buffer, size_t count, void *opaque) {
    yamaa_ctx *ctx = opaque;
    /* The transport charges bytes actually read. A short malformed source is
     * a short read, not a refusal of the parser's larger header request. */
    if (count > ctx->length - ctx->position) count = ctx->length - ctx->position;
    if (!admit_operation(ctx)) return -1;
    if (count > ctx->read_limit - ctx->reads) {
        refuse(6, ctx->read_limit, ctx->reads + count, count <= SIZE_MAX - ctx->reads); return -1;
    }
    /* The measured graph route admits its cumulative actual snapshot reads
     * before touching the buffer. Legacy public parsing keeps its IO policy. */
    if (ctx->measured && count > work_limit - ctx->reads) {
        refuse(4, work_limit, ctx->reads + count, count <= SIZE_MAX - ctx->reads); return -1;
    }
    if (ctx->measured) charge_work(ctx->reads + count);
    memcpy(buffer, ctx->bytes + ctx->position, count);
    ctx->position += count;
    ctx->reads += count;
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
        size_t rows, size_t columns, size_t cells, size_t work,
        int measured, size_t *work_used,
        yamaa_metadata metadata, yamaa_variable variable, yamaa_value value,
        void *user, int *resource, size_t *limit, size_t *actual, int *known) {
    yamaa_ctx ctx = { .bytes = bytes, .length = length, .metadata = metadata,
        .variable = variable, .value = value, .user = user, .xport = xport,
        .measured = measured };
    ctx.read_limit = saturated_mul(length, 4);
    size_t read_budget = measured && work < ctx.read_limit ? work : ctx.read_limit;
    /* Every nonempty read consumes at least one byte. Page seeks are paired
     * with reads; XPT labels add at most four empty reads per column. Eight
     * operations per column and 64 header/EOF operations cover that fixed
     * framing. Admitted records add their separately bounded row geometry. */
    ctx.operation_limit = saturated_add(saturated_mul(read_budget, 2),
        saturated_add(rows < cells ? rows : cells,
            saturated_add(saturated_mul(columns, 8), 64)));
    rows_limit = rows; columns_limit = columns; cells_limit = cells;
    work_limit = work; work_remaining = work; limit_hit = 0;
    readstat_parser_t *parser = readstat_parser_init();
    if (!parser) { *work_used = work_limit - work_remaining; return -6; }
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
    *work_used = work_limit - work_remaining;
    if (limit_hit) {
        *resource = refusal_resource; *limit = refusal_limit;
        *actual = refusal_actual; *known = refusal_known;
        return -2;
    }
    if (status == READSTAT_ERROR_MALLOC) return -6;
    if (status == READSTAT_ERROR_UNSUPPORTED_COMPRESSION) return -3;
    if (status == READSTAT_ERROR_CONVERT_BAD_STRING) return -4;
    if (status == READSTAT_ERROR_UNSUPPORTED_FILE_FORMAT_VERSION) return -5;
    return status;
}
