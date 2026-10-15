
void *readstat_malloc(size_t size);
void *readstat_calloc(size_t count, size_t size);
void *readstat_realloc(void *ptr, size_t len);

/* Local reader admission keeps the previous owned allocation on any failure.
 * All selected readers use these macros instead of assigning realloc directly. */
#define READSTAT_RESIZE_OR_RETURN(pointer, length) do { \
    void *readstat_replacement = readstat_realloc((pointer), (length)); \
    if (!readstat_replacement) return READSTAT_ERROR_MALLOC; \
    (pointer) = readstat_replacement; \
} while (0)
#define READSTAT_RESIZE_OR_CLEANUP(pointer, length) do { \
    void *readstat_replacement = readstat_realloc((pointer), (length)); \
    if (!readstat_replacement) { retval = READSTAT_ERROR_MALLOC; goto cleanup; } \
    (pointer) = readstat_replacement; \
} while (0)
