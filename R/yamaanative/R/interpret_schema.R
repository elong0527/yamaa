#' Interpret an experimental decoded schema snapshot through shared Rust
#'
#' Accepts schema/1 JSON with decoded modules and a query batch. Schema defects,
#' validation findings, unsupported shapes and policy refusals are data outcomes.
#' YAML decoding and filesystem access are caller responsibilities. This service
#' does not enable workflow execution or change the default engine.
#' @param request One unclassed, nonmissing JSON character string without attributes.
#' @return Owned outcome JSON; malformed transport raises an R error after Rust returns.
#' @export
interpret_schema <- function(request) {
  if (!is.character(request) || length(request) != 1L || is.na(request)) {
    stop("request must be one nonmissing JSON character string", call. = FALSE)
  }
  result <- .Call(wrap__interpret_schema, .scalar_text_bytes(request))
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
