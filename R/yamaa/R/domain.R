# Public host conversion; file preparation, evaluation and publication are native.
.domain_reply <- function(reply) {
  if (!is.null(reply$error)) stop(reply$error, call. = FALSE)
  reply$value
}
.domain_argument_error <- function(message) {
  stop(structure(list(message = message, call = NULL),
    class = c("yamaa_argument_error", "error", "condition")))
}
.domain_path_bytes <- function(value) {
  if (!is.character(value) || length(value) != 1L || is.na(value) ||
      !is.null(attributes(value))) {
    .domain_argument_error("expected one unclassed non-missing character scalar")
  }
  if (identical(Encoding(value), "bytes")) {
    .domain_argument_error("byte-marked R text has no declared Unicode encoding")
  }
  # Keep conversion bounded. An oversized prefix still exceeds the native
  # 65536-byte path limit and is rejected as an issue before any file access.
  # Every admitted path is passed in full, including its declared encoding.
  .scalar_text_bytes(substr(value, 1L, 65537L))
}
.domain_arguments <- function(specification, environment) {
  list(.domain_path_bytes(specification),
       if (is.null(environment)) NULL else .domain_path_bytes(environment),
       charToRaw(as.character(getRversion())))
}

# Private qualification reads the same handle retained by the public save closure.
# This is transport of Rust observations, with no host semantic preparation.
.domain_observations <- function(result, include_measurements = FALSE) {
  handle <- get("h", envir = environment(result$save), inherits = FALSE)
  .domain_reply(.Call(wrap__domain_observations, handle, include_measurements))
}

#' Build a domain through the shared Rust engine
#' @param specification One domain-specification path.
#' @param environment Optional study environment path for functions and codelists.
#' @return An owned result with output, issues, declared logs and a save method.
#' @export
yamaa_domain <- function(specification, environment = NULL) {
  args <- .domain_arguments(specification, environment)
  capabilities <- .yamaa_locked_host_capabilities()
  handle <- .domain_reply(.Call(wrap__domain_file, args[[1L]], args[[2L]], args[[3L]],
    list(capabilities$verify, capabilities$resolve, .project_condition_details)))
  interrupted <- .domain_reply(.Call(wrap__domain_interrupt, handle))
  if (!is.null(interrupted)) stop(interrupted)
  result <- new.env(parent = emptyenv())
  makeActiveBinding("output", local({ h <- handle; function(value) {
    if (!missing(value)) stop("output is read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_output, h))
  }}), result)
  makeActiveBinding("issues", local({ h <- handle; function(value) {
    if (!missing(value)) stop("issues are read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_issues, h))
  }}), result)
  makeActiveBinding("verification_log", local({ h <- handle; function(value) {
    if (!missing(value)) stop("verification_log is read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_log, h, FALSE))
  }}), result)
  makeActiveBinding("warning_log", local({ h <- handle; function(value) {
    if (!missing(value)) stop("warning_log is read only", call. = FALSE)
    .domain_reply(.Call(wrap__domain_log, h, TRUE))
  }}), result)
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

#' Convert lossless UTF-8 output to character only when every value is exact
#' @param x A yamaa UTF-8 output vector.
#' @param ... Unused.
#' @export
as.character.yamaa_utf8_vector <- function(x, ...) {
  if (!identical(class(x), "yamaa_utf8_vector")) {
    stop("invalid UTF-8 output vector class", call. = FALSE)
  }
  values <- unclass(x)
  if (!is.list(values) || any(!(names(attributes(values)) %in% "names"))) {
    stop("invalid UTF-8 output vector storage", call. = FALSE)
  }
  vapply(values, function(value) {
    if (is.null(value)) return(NA_character_)
    if (!is.raw(value) || !is.null(attributes(value))) {
      stop("invalid UTF-8 output value storage", call. = FALSE)
    }
    # Reuse strict scalar admission and its preserve-or-error R conversion.
    as.character(.scalar_unpack(.scalar_native(3L, value)))
  }, character(1L))
}

#' @export
`[.yamaa_int64_vector` <- function(x, ...) structure(unclass(x)[...], class = "yamaa_int64_vector")

#' @export
is.na.yamaa_utf8_vector <- function(x) vapply(unclass(x), is.null, logical(1))

#' @export
`[.yamaa_utf8_vector` <- function(x, ...) structure(unclass(x)[...], class = "yamaa_utf8_vector")
