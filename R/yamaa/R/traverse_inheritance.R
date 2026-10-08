#' Traverse inherited layers through shared Rust with explicit source authority
#'
#' The inheritance/1 service normalizes each layer, checks versions and visits
#' parents in deterministic postorder. The callback implements only canonicalize
#' and read operations from the closed protocol. It receives request JSON and
#' the remaining cumulative reply-byte allowance, and returns reply JSON.
#' No filesystem reader is supplied implicitly. Original callback errors and
#' interruptions are raised after Rust returns. This does not enable execution.
#' @param schema_request One schema/1 compile request as JSON text.
#' @param request One inheritance/1 entry request as JSON text.
#' @param callback Explicit function taking request text and maximum reply bytes.
#' @return Owned inheritance/1 outcome JSON with normalized contributions or failure.
#' @export
traverse_inheritance <- function(schema_request, request, callback) {
  if (!is.function(callback)) stop("callback must be a function", call. = FALSE)
  force(callback)
  failure <- NULL
  dispatch <- function(request, maximum) tryCatch({
    response <- callback(rawToChar(request), maximum)
    bytes <- .scalar_text_bytes(response)
    if (length(bytes) > maximum) {
      stop("inheritance source reply exceeds byte limit", call. = FALSE)
    }
    list(0L, bytes)
  }, error = function(e) {
    failure <<- e
    list(1L, NULL)
  }, interrupt = function(e) {
    failure <<- e
    list(1L, NULL)
  })
  result <- .Call(
    wrap__traverse_inheritance,
    .scalar_text_bytes(schema_request), .scalar_text_bytes(request), dispatch
  )
  if (!is.null(failure)) stop(failure)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}
