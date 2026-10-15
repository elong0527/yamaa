/* yamaa portability shim: ReadStat emits raw source bytes. Rust alone decodes
 * text; an unexpected request to use platform iconv must fail closed. */
#ifndef YAMAA_RAW_ICONV_H
#define YAMAA_RAW_ICONV_H
#include <stddef.h>
#include <errno.h>
typedef void *iconv_t;
static inline iconv_t iconv_open(const char *to, const char *from) {
    (void)to; (void)from; errno = EINVAL; return (iconv_t)-1;
}
static inline int iconv_close(iconv_t converter) { (void)converter; return 0; }
static inline size_t iconv(iconv_t converter, char **src, size_t *src_len,
        char **dst, size_t *dst_len) {
    (void)converter; (void)src; (void)src_len; (void)dst; (void)dst_len;
    errno = EINVAL; return (size_t)-1;
}
#endif
