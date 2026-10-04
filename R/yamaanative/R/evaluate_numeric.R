#' Evaluate one bounded numeric request through the shared Rust engine
#'
#' Accepts a numeric/1 JSON request with normalized scalar bindings and an
#' optional literal replacement. Returns structured value, failure, unsupported
#' or resource-limit JSON; never falls back to another evaluator. This prototype
#' does not execute specifications, tables, project callbacks or output writes.
#' Requests have no attributes. Declared Latin-1 is decoded explicitly; byte-marked
#' text and malformed UTF-8 are rejected. Raw bytes are validated in Rust before
#' constructing a string, without locale fallback or replacement.
#' @param request One unclassed, nonmissing JSON character string.
#' @return Owned outcome JSON. Malformed transport raises an R error after the
#'   native call returns. Language failures are structured outcomes.
#' @export
evaluate_numeric <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__evaluate_numeric, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
