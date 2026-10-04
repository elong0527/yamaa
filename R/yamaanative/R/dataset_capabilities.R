#' Discover typed dataset features
#'
#' Reports the shared typed-plan protocol and additive features before reading
#' source data. This does not qualify specification execution or change defaults.
#' @return JSON text with a protocol string and a features array.
#' @export
dataset_capabilities <- function() {
  .Call(wrap__dataset_capabilities)
}
