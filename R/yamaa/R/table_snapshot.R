#' Copy or inspect a lossless Arrow table through Rust
#'
#' Uses a bounded canonical IPC stream and preserves full i64 and temporal precision.
#' This is table interchange, not specification execution.
#' @param request A raw vector containing one canonical Arrow IPC stream.
#' @return One owned table/1 JSON string with exact typed scalar values.
#' @export
table_snapshot <- function(request) {
  if (!is.raw(request)) {
    stop("request must be a raw Arrow IPC vector", call. = FALSE)
  }
  result <- .Call(wrap__table_snapshot, request)
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
