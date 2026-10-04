#' Execute a bound dataset plan with secondary sources through Rust
#'
#' This optional dataset/1 bridge accepts a closed typed plan and canonical Arrow
#' IPC. It does not compile specifications, activate callbacks, read files or
#' publish artifacts. Unsupported features must be rejected by the caller's
#' compiler before execution; there is no fallback. All work stays on the R thread.
#' @param request One unclassed, nonmissing JSON character string.
#' @param source A raw vector containing the canonical source IPC snapshot.
#' @param secondary A list of raw IPC vectors in declared secondary-source order.
#' @return A list with `table` (owned raw IPC on success, otherwise NULL) and
#'   `outcome` (exact JSON observations). Transport errors raise after Rust returns.
#' @export
execute_dataset_sources <- function(request, source, secondary) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  if (!is.raw(source)) {
    stop("source must be a raw Arrow IPC vector", call. = FALSE)
  }
  if (!is.list(secondary) || any(!vapply(secondary, is.raw, logical(1)))) {
    stop("secondary must be a list of raw Arrow IPC vectors", call. = FALSE)
  }
  result <- .Call(wrap__execute_dataset_sources, .scalar_text_bytes(request), source, secondary)
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
