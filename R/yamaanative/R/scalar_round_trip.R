#' Round-trip one lossless scalar envelope through Rust
#'
#' This boundary probe decodes a versioned JSON envelope to a core value and
#' returns owned JSON text. It does not evaluate specifications or convert the
#' result to an ordinary R numeric vector. Full i64 values use decimal strings;
#' floats use exact binary64 bits. See the package README for the wire format.
#' Requests have no attributes. Declared Latin-1 is decoded explicitly; byte-marked
#' text and malformed UTF-8 are rejected. Raw bytes are validated in Rust before
#' constructing a string, without locale fallback or replacement.
#' @param request One unclassed, nonmissing JSON character string.
#' @return One owned JSON character string. Invalid requests raise an error.
#' @export
scalar_round_trip <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__scalar_round_trip, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
