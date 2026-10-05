#' Execute a typed dataset with explicit callbacks through Rust
#'
#' The callback list follows the request's function declaration order. Callables
#' are captured for one synchronous run; no discovery, activation, retry or
#' rollback is performed. A failed run exposes no accepted table, but may already
#' have invoked callbacks. This optional bridge does not compile specifications.
#' @param request One unclassed, nonmissing JSON character string.
#' @param source A raw vector containing canonical source IPC.
#' @param secondary A list of raw secondary IPC vectors in declaration order.
#' @param callbacks A list of explicitly supplied functions in declaration order.
#' @return A list with owned `table` IPC on success and exact `outcome` JSON.
#'   Transport failures and original interruptions raise after native return.
#' @export
execute_dataset_functions <- function(request, source, secondary, callbacks) {
  if (!is.raw(source)) stop("source must be a raw Arrow IPC vector", call. = FALSE)
  if (!is.list(secondary) || any(!vapply(secondary, is.raw, logical(1)))) {
    stop("secondary must be a list of raw Arrow IPC vectors", call. = FALSE)
  }
  if (!is.list(callbacks) || length(callbacks) > 64L ||
      any(!vapply(callbacks, is.function, logical(1)))) {
    stop("callbacks must be a list of at most 64 functions", call. = FALSE)
  }
  states <- lapply(callbacks, .function_dispatcher)
  dispatchers <- lapply(states, function(state) state$dispatch)
  result <- .Call(wrap__execute_dataset_functions, .scalar_text_bytes(request),
                  source, secondary, dispatchers)
  for (state in states) {
    interrupted <- state$interruption()
    if (!is.null(interrupted)) stop(interrupted)
  }
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}
