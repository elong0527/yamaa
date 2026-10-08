#' Analyze one already-bound dependency graph through shared Rust
#'
#' Accepts dependency-analysis/1 JSON with graph nodes in declaration order.
#' Returns a canonical cycle path and stable scheduling order, or an explicit
#' resource-limit outcome. This does not resolve names, load data, invoke user
#' functions or compile a specification. Python is not involved in this call.
#' @param request One unclassed, nonmissing JSON character string without attributes.
#' @return Owned outcome JSON. Malformed transport raises an R error only after
#'   the native call returns. Graph cycles and resource limits are data outcomes.
#' @export
analyze_dependencies <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__analyze_dependencies, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
