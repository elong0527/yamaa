#' Inspect the native installation prototype
#'
#' Calls the shared Rust application boundary. This probe does not implement
#' specification execution and always reports that limitation explicitly.
#' @return A named list containing the core version, probe protocol version,
#'   execution support flag, and embedded installation resource.
#' @export
engine_info <- function() {
  .Call(wrap__engine_info)
}
