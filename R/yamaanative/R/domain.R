# Public host conversion; file preparation, evaluation and publication are native.
.domain_reply <- function(reply) {
  if (!is.null(reply$error)) stop(reply$error, call. = FALSE)
  reply$value
}
.domain_arguments <- function(specification, environment) {
  list(.specification_text_bytes(specification, 65536),
       if (is.null(environment)) NULL else .specification_text_bytes(environment, 65536),
       charToRaw(as.character(getRversion())))
}

#' Build a domain through the shared Rust engine
#' @param specification One domain-specification path.
#' @param environment Optional environment path. Unsupported environments are issues.
#' @return An owned result with output, issues, declared logs and a save method.
#' @export
yamaa_domain <- function(specification, environment = NULL) {
  args <- .domain_arguments(specification, environment)
  handle <- .domain_reply(.Call(wrap__domain_file, args[[1L]], args[[2L]], args[[3L]]))
  result <- new.env(parent = emptyenv())
  makeActiveBinding("output", local({ h <- handle; function(value) {
    if (!missing(value)) stop("output is read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_output, h))
  }}), result)
  makeActiveBinding("issues", local({ h <- handle; function(value) {
    if (!missing(value)) stop("issues are read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_issues, h))
  }}), result)
  result$verification_log <- NULL
  result$warning_log <- NULL
  result$save <- local({ h <- handle; function() {
    reply <- .domain_reply(.Call(wrap__domain_save, h))
    if (reply$failed) stop(structure(list(message = "cannot save a failed build", call = NULL),
      class = c("yamaa_domain_error", "error", "condition")))
    reply$saved
  }})
  lockEnvironment(result, bindings = TRUE)
  result
}

#' Check a specification without reading study data or running project code
#' @inheritParams yamaa_domain
#' @return A list containing the issues data frame.
#' @export
yamaa_check <- function(specification, environment = NULL) {
  args <- .domain_arguments(specification, environment)
  list(issues = .domain_reply(.Call(wrap__check_file, args[[1L]], args[[2L]], args[[3L]])))
}

#' Copy lossless signed 64-bit output values as canonical text
#' @param x A yamaa integer output vector.
#' @param ... Unused.
#' @export
as.character.yamaa_int64_vector <- function(x, ...) unclass(x)

#' @export
`[.yamaa_int64_vector` <- function(x, ...) structure(unclass(x)[...], class = "yamaa_int64_vector")

#' @export
is.na.yamaa_utf8_vector <- function(x) vapply(unclass(x), is.null, logical(1))

#' @export
`[.yamaa_utf8_vector` <- function(x, ...) structure(unclass(x)[...], class = "yamaa_utf8_vector")
