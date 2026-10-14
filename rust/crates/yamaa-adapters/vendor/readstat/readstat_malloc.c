#include <stdlib.h>
#include <stdint.h>
extern int yamaa_readstat_allocation(size_t, int);

/* The memory bridge admits every checked allocation against the caller's work
 * allowance. Format readers validate source extents and physical dimensions
 * first; a second fixed ceiling would reject valid wide observations. */

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
