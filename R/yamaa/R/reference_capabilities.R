#' Discover shared reference compiler queries
#'
#' Reports implemented metadata queries before catalog preparation or source
#' access. This does not qualify current-schema execution or change defaults.
#' @return JSON text with a protocol string and a features array.
#' @export
reference_capabilities <- function() {
  .Call(wrap__reference_capabilities)
}
