#' Execute one explicitly bound dataset plan through Rust
#'
#' This optional dataset/1 bridge accepts a closed typed plan and canonical Arrow
#' IPC. It does not compile specifications, activate callbacks, read files or
#' publish artifacts. Unsupported features must be rejected by the caller's
#' compiler before execution; there is no fallback. All work stays on the R thread.
#' @param request One unclassed, nonmissing JSON character string.
#' @param source A raw vector containing the canonical source IPC snapshot.
#' @return A list with `table` (owned raw IPC on success, otherwise NULL) and
#'   `outcome` (exact JSON observations). Transport errors raise after Rust returns.
#' @export
execute_dataset <- function(request, source) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  if (!is.raw(source)) {
    stop("source must be a raw Arrow IPC vector", call. = FALSE)
  }
  result <- .Call(wrap__execute_dataset, .scalar_text_bytes(request), source)
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
