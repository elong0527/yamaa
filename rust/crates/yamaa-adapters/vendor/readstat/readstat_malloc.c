#include <stdlib.h>
#include <stdint.h>

void *readstat_malloc(size_t len) {
    return len == 0 ? NULL : malloc(len);
}

void *readstat_calloc(size_t count, size_t size) {
    if ((size && count > SIZE_MAX / size) || count == 0 || size == 0) return NULL;
    return calloc(count, size);
}

void *readstat_realloc(void *ptr, size_t len) {
    return len == 0 ? NULL : realloc(ptr, len);
}
