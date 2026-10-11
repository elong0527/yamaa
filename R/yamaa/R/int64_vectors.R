# Convert each held decimal value through the shared exact scalar boundary.
# Character NA remains separate from every signed 64-bit value.
.int64_vector_convert <- function(x, convert, missing_value) {
  if (!identical(class(x), "yamaa_int64_vector")) {
    stop("invalid integer output vector class", call. = FALSE)
  }
  values <- unclass(x)
  if (!is.character(values) ||
      any(!(names(attributes(values)) %in% "names"))) {
    stop("invalid integer output vector storage", call. = FALSE)
  }
  result <- vapply(values, function(value) {
    if (is.na(value)) missing_value else convert(int64(value))
  }, missing_value, USE.NAMES = FALSE)
  names(result) <- names(values)
  result
}

#' Convert lossless integer output to double only when every value is exact
#' @param x A yamaa integer output vector.
#' @param ... Unused.
#' @return A double vector preserving names and missingness, or an error.
#' @export
as.double.yamaa_int64_vector <- function(x, ...) {
  .int64_vector_convert(x, as.double, NA_real_)
}

#' Convert lossless integer output only within the non-missing R integer range
#' @inheritParams as.double.yamaa_int64_vector
#' @return An integer vector preserving names and missingness, or an error.
#' @export
as.integer.yamaa_int64_vector <- function(x, ...) {
  .int64_vector_convert(x, as.integer, NA_integer_)
}
