#' Parse numeric syntax through shared Rust
#'
#' Accepts numeric-syntax/1 JSON containing an expression string. Returns an
#' owned AST with exact number text, ordered references, or portable
#' grammar/resource diagnostics. This parses the closed grammar without binding
#' names, reading data or granting execution support. No Python is involved.
#' @param request One unclassed, nonmissing JSON character string without attributes.
#' @return Owned outcome JSON. Malformed transport raises an R error only after
#'   the native call returns; grammar failures and resource limits are data.
#' @export
analyze_numeric <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__analyze_numeric, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
