#include <stdlib.h>
#include <stdint.h>
extern int yamaa_readstat_allocation(size_t, int);

#define MAX_MALLOC_SIZE 0x1000000
/* =16 MiB. Needs to be at least 0x3FF00, i.e. the default ~4MB block size used
 * in compressed SPSS (ZSAV) files. Some SAS installations use 16MiB page sizes
 * by default, see https://github.com/tidyverse/haven/issues/697.
 * The purpose here is to prevent massive allocations in the event of a
 * malformed file or a bug in the library. */

void *readstat_malloc(size_t len) {
    if (!yamaa_readstat_allocation(len, 1) || len == 0) {
        return NULL;
    }
    return malloc(len);
}

void *readstat_calloc(size_t count, size_t size) {
    if ((size && count > SIZE_MAX / size) || !yamaa_readstat_allocation(count * size, 1)) {
        if (size && count > SIZE_MAX / size) yamaa_readstat_allocation(0, 0);
        return NULL;
    }
    if (count == 0 || size == 0) {
        return NULL;
    }
    return calloc(count, size);
}

void *readstat_realloc(void *ptr, size_t len) {
    if (!yamaa_readstat_allocation(len, 1) || len == 0) {
        return NULL;
    }
    return realloc(ptr, len);
}
