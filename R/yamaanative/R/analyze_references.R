#' Bind and validate references through shared Rust compiler metadata
#'
#' Accepts reference-analysis/1 JSON containing an ordered typed catalog and
#' query batch. Resolves exact output/dataset references and validates bare
#' output names, suggestions, phase availability and expected types.
#' Also checks normalized direct qualified-field driver, scope and grouping
#' metadata. Discover query support with reference_capabilities().
#' No records, callbacks or Python installation are involved.
#' @param request One unclassed, nonmissing JSON character string without attributes.
#' @return Owned outcome JSON. Language diagnostics and resource limits are data;
#'   invalid transport raises an error only after the native call returns.
#' @export
analyze_references <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__analyze_references, .scalar_text_bytes(request))
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}
