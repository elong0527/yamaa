# Strict host representations for existing int/str logical scalars. R character
# bytes are admitted as raw payloads; Rust never borrows an unchecked R string.
#' Encode declared R text without locale fallback or Unicode repair.
.scalar_text_bytes <- function(x) {
  if (!is.character(x) || length(x) != 1L || is.na(x) || !is.null(attributes(x))) {
    stop("expected one unclassed non-missing character scalar", call. = FALSE)
  }
  encoding <- Encoding(x)
  if (identical(encoding, "bytes")) {
    stop("byte-marked R text has no declared Unicode encoding", call. = FALSE)
  }
  # A declared Latin-1 character has one defined Unicode decoding. Unmarked or
  # UTF-8 text is passed as held bytes for strict UTF-8 validation, never repaired.
  if (identical(encoding, "latin1")) x <- enc2utf8(x)
  charToRaw(x)
}

#' Validate raw bytes and raise only after the native call returns.
.scalar_native <- function(tag, payload) {
  result <- .Call(wrap__scalar_bytes, tag, payload)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result
}

#' Admit exact scalar classes and normalize primitive missing values.
.scalar_pack <- function(x) {
  attributes <- attributes(x)
  if (identical(attributes, list(class = "yamaa_int64"))) {
    bytes <- .scalar_text_bytes(unclass(x))
    return(.scalar_native(1L, bytes))
  }
  if (identical(attributes, list(class = "yamaa_utf8"))) {
    bytes <- unclass(x)
    if (!is.raw(bytes)) stop("invalid UTF-8 scalar storage", call. = FALSE)
    return(.scalar_native(3L, bytes))
  }
  if (!is.null(attributes) || length(x) != 1L) {
    stop("expected one supported scalar without extra attributes", call. = FALSE)
  }
  kind <- typeof(x)
  if (!(kind %in% c("integer", "double", "logical", "character"))) {
    stop("unsupported scalar type", call. = FALSE)
  }
  if (is.na(x) || (identical(kind, "double") && !is.finite(x))) {
    return(.scalar_native(0L, raw()))
  }
  switch(kind,
    integer = .scalar_native(1L, charToRaw(as.character(x))),
    double = .scalar_native(2L, writeBin(x, raw(), size = 8L, endian = "little")),
    logical = .scalar_native(4L, as.raw(as.integer(x))),
    character = .scalar_native(3L, .scalar_text_bytes(x))
  )
}

#' Construct an owned host scalar from a validated native payload.
.scalar_unpack <- function(value) {
  if (!is.null(value$error)) stop(value$error, call. = FALSE)
  switch(as.character(value$tag),
    `0` = NA,
    `1` = structure(rawToChar(value$payload), class = "yamaa_int64"),
    `2` = readBin(value$payload, "double", n = 1L, size = 8L, endian = "little"),
    `3` = structure(value$payload, class = "yamaa_utf8"),
    `4` = identical(value$payload, as.raw(1L)),
    `5` = structure(readBin(value$payload, "double", n = 1L, size = 8L,
                            endian = "little"), class = "Date"),
    `6` = structure(readBin(value$payload, "double", n = 1L, size = 8L,
                            endian = "little"), class = c("POSIXct", "POSIXt"),
                     tzone = "UTC"),
    stop("invalid native scalar tag", call. = FALSE)
  )
}

#' Construct a lossless signed 64-bit scalar
#'
#' Canonical decimal text preserves the complete i64 range without an NA sentinel.
#' Ordinary R integer scalars are accepted; double input is rejected to prevent
#' already-rounded values from being mistaken for exact integers.
#' @param x One canonical decimal character scalar or non-missing R integer.
#' @return A validated `yamaa_int64` scalar.
#' @export
int64 <- function(x) {
  if (is.integer(x) && is.null(attributes(x)) && length(x) == 1L && !is.na(x)) {
    x <- as.character(x)
  }
  .scalar_unpack(.scalar_native(1L, .scalar_text_bytes(x)))
}

#' Construct a lossless UTF-8 scalar
#'
#' Raw storage preserves embedded NUL and every Unicode scalar. Invalid UTF-8 is
#' rejected rather than replaced. Missing is distinct from empty text.
#' @param x An unclassed raw UTF-8 vector or one non-missing character scalar.
#' @return A validated `yamaa_utf8` scalar.
#' @export
utf8_scalar <- function(x) {
  if (is.raw(x) && is.null(attributes(x))) {
    bytes <- x
  } else {
    bytes <- .scalar_text_bytes(x)
  }
  .scalar_unpack(.scalar_native(3L, bytes))
}

#' Access the exact UTF-8 bytes of a scalar
#' @param x A `yamaa_utf8` or ordinary character scalar.
#' @return An owned raw UTF-8 vector, including any NUL bytes.
#' @export
utf8_bytes <- function(x) {
  value <- .scalar_pack(x)
  if (!identical(value$tag, 3L)) stop("expected a text scalar", call. = FALSE)
  value$payload
}

#' Return canonical decimal text without numeric conversion.
#' @export
as.character.yamaa_int64 <- function(x, ...) {
  value <- .scalar_pack(x)
  rawToChar(value$payload)
}

#' Format a validated integer as its exact decimal spelling.
#' @export
format.yamaa_int64 <- function(x, ...) as.character(x)

#' Print an integer without narrowing its value.
#' @export
print.yamaa_int64 <- function(x, ...) {
  cat("<yamaa_int64> ", as.character(x), "\n", sep = "")
  invisible(x)
}

#' Convert to binary64 only when the integer is exactly retained.
#' @export
as.double.yamaa_int64 <- function(x, ...) {
  value <- .scalar_pack(x)
  .scalar_unpack(.Call(wrap__scalar_exact_double, value$tag, value$payload))
}

#' Convert only inside the non-missing R integer range.
#' @export
as.integer.yamaa_int64 <- function(x, ...) {
  if (x < int64("-2147483647") || x > int64("2147483647")) {
    stop("integer is not representable as a non-missing R integer", call. = FALSE)
  }
  as.integer(as.double(x))
}

#' Return native text only when R can preserve every byte.
#' @export
as.character.yamaa_utf8 <- function(x, ...) {
  bytes <- utf8_bytes(x)
  if (any(bytes == as.raw(0L))) {
    stop("R character cannot preserve NUL; use utf8_bytes", call. = FALSE)
  }
  text <- rawToChar(bytes)
  Encoding(text) <- "UTF-8"
  text
}

#' Format exact bytes without changing the held UTF-8 scalar.
#' @export
format.yamaa_utf8 <- function(x, ...) {
  bytes <- utf8_bytes(x)
  paste0("<yamaa_utf8: ", paste(format(bytes), collapse = " "), ">")
}

#' Print the lossless byte representation of a text scalar.
#' @export
print.yamaa_utf8 <- function(x, ...) {
  cat(format(x), "\n", sep = "")
  invisible(x)
}

#' Dispatch a closed scalar operator without vector recycling.
.scalar_ops <- function(e1, e2) {
  unary <- missing(e2)
  code <- if (unary) switch(.Generic, `+` = 6L, `-` = 7L) else {
    switch(.Generic, `+` = 1L, `-` = 2L, `*` = 3L, `/` = 4L,
           `==` = 8L, `!=` = 9L, `<` = 10L,
           `<=` = 11L, `>` = 12L, `>=` = 13L)
  }
  if (is.null(code)) stop("unsupported scalar operator", call. = FALSE)
  left <- .scalar_pack(e1)
  right <- if (unary) .scalar_native(0L, raw()) else .scalar_pack(e2)
  .scalar_unpack(.Call(wrap__scalar_operator, code, left$tag, left$payload,
                       right$tag, right$payload, unary))
}

#' @export
Ops.yamaa_int64 <- .scalar_ops
#' @export
Ops.yamaa_utf8 <- .scalar_ops

#' Validate the present integer before reporting non-missingness.
#' @export
is.na.yamaa_int64 <- function(x) {
  .scalar_pack(x)
  FALSE
}

#' Validate the present text before reporting non-missingness.
#' @export
is.na.yamaa_utf8 <- function(x) {
  .scalar_pack(x)
  FALSE
}

#' Report one logical scalar independently of its byte count.
#' @export
length.yamaa_utf8 <- function(x) {
  .scalar_pack(x)
  1L
}
