#include <R_ext/Rdynload.h>
#include <R_ext/Visibility.h>

void R_init_yamaanative_extendr(DllInfo *dll);

void attribute_visible R_init_yamaanative(DllInfo *dll) {
    R_init_yamaanative_extendr(dll);
}
