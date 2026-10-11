# Convert each held decimal value through the shared exact scalar boundary.
# Character NA remains separate from every signed 64-bit value.
.int64_vector_storage <- function(x) {
  if (!identical(class(x), "yamaa_int64_vector")) {
    stop("invalid integer output vector class", call. = FALSE)
  }
  values <- unclass(x)
  if (!is.character(values) ||
      any(!(names(attributes(values)) %in% "names"))) {
    stop("invalid integer output vector storage", call. = FALSE)
  }
  values
}

.int64_vector_convert <- function(x, convert, missing_value) {
  values <- .int64_vector_storage(x)
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

#' Extract one lossless integer output value with its separate missingness
#' @param x A yamaa integer output vector.
#' @param ... Subscript arguments passed to ordinary character extraction.
#' @return A one-element yamaa integer output vector.
#' @export
`[[.yamaa_int64_vector` <- function(x, ...) {
  structure(.int64_vector_storage(x)[[...]], class = "yamaa_int64_vector")
}

.int64_vector_operand <- function(x) {
  if (inherits(x, "yamaa_int64_vector")) {
    values <- .int64_vector_storage(x)
    scalar <- function(index) {
      value <- unname(values[index])
      if (is.na(value)) .scalar_native(0L, raw()) else {
        .scalar_native(1L, .scalar_text_bytes(value))
      }
    }
  } else if (identical(class(x), "yamaa_int64")) {
    value <- .scalar_pack(x)
    return(list(length = 1L, names = NULL, scalar = function(index) value))
  } else {
    if (!is.null(x) &&
        (!(typeof(x) %in% c("integer", "double", "logical", "character")) ||
         any(!(names(attributes(x)) %in% "names")))) {
      stop("unsupported integer output comparison operand", call. = FALSE)
    }
    values <- x
    scalar <- function(index) .scalar_pack(unname(values[index]))
  }
  list(length = length(values), names = names(values), scalar = scalar)
}

.int64_vector_ops <- function(e1, e2, operator) {
  code <- switch(operator, `==` = 8L, `!=` = 9L, `<` = 10L,
                 `<=` = 11L, `>` = 12L, `>=` = 13L)
  if (missing(e2) || is.null(code)) {
    stop("unsupported integer output vector operator", call. = FALSE)
  }
  left <- .int64_vector_operand(e1)
  right <- .int64_vector_operand(e2)
  if (left$length == 0L || right$length == 0L) return(logical())
  count <- max(left$length, right$length)
  if (count %% min(left$length, right$length) != 0L) {
    warning("longer object length is not a multiple of shorter object length",
            call. = FALSE)
  }
  result <- vapply(seq_len(count), function(index) {
    a <- left$scalar((index - 1L) %% left$length + 1L)
    b <- right$scalar((index - 1L) %% right$length + 1L)
    .scalar_unpack(.Call(wrap__scalar_operator, code, a$tag, a$payload,
                        b$tag, b$payload, FALSE))
  }, logical(1L), USE.NAMES = FALSE)
  names(result) <- if (left$length == count && !is.null(left$names)) {
    left$names
  } else if (right$length == count) right$names else NULL
  result
}
