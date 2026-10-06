#' Compile or match a portable regular expression through Rust
#'
#' Accepts regex/1 JSON with a pattern and a compile, search or full_match
#' operation. Compile-only requests contain no subject. Captures distinguish
#' empty text, an unentered group and no match. Invalid patterns and resource
#' refusals are structured outcomes. This opt-in service grants no dataset
#' execution capability and uses no Python or host regex library.
#' @param request One unclassed, nonmissing JSON character string without attributes.
#' @return Owned outcome JSON. Malformed transport raises an R error only after
#'   the native call returns. Requests and serialized responses are bounded.
#' @export
evaluate_regex <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__evaluate_regex, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
