#' Decode an ASCII YAML snapshot through shared Rust
#'
#' Accepts raw bytes and returns yaml/1 JSON. Scalar integers remain decimal
#' strings and floats remain exact bits in an ordered document arena; no R
#' numeric conversion or yaml12 parsing occurs. Invalid source and resource
#' refusals are explicit outcomes. This does not enable workflow execution.
#' @param source An unclassed raw vector without attributes containing one
#'   retained ASCII YAML snapshot.
#' @return Owned outcome JSON. Transport failures raise an R error after Rust
#'   returns; source diagnostics contain no partial decoded document.
#' @export
decode_yaml <- function(source) {
  if (!is.raw(source) || !is.null(attributes(source))) {
    stop("source must be a raw vector without attributes", call. = FALSE)
  }
  result <- .Call(wrap__decode_yaml, source)
  if (!is.null(result$error)) {
    stop(result$error, call. = FALSE)
  }
  result$value
}
