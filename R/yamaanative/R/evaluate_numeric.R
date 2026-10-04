#' Evaluate one bounded numeric request through the shared Rust engine
#'
#' Accepts a numeric/1 JSON request with normalized scalar bindings and an
#' optional literal replacement. Returns structured value, failure, unsupported
#' or resource-limit JSON; never falls back to another evaluator. This prototype
#' does not execute specifications, tables, project callbacks or output writes.
#' @param request One nonmissing UTF-8 JSON character string.
#' @return Owned outcome JSON. Malformed transport raises an R error after the
#'   native call returns. Language failures are structured outcomes.
#' @export
evaluate_numeric <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__evaluate_numeric, enc2utf8(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
