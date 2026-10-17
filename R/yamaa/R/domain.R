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
  .scalar_text_bytes(value)
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
  .domain_reply(.native_domain_observations(handle, include_measurements))
}

#' Build a domain through the shared Rust engine
#' @param specification One domain-specification path.
#' @param environment Optional study environment path for functions and codelists.
#' @param resolver Optional host resource resolver.
#' @return An owned result with output, issues, diagnostics, declared logs and a
#'   save method. `issues` is the data-frame projection of the same retained
#'   diagnostics that `diagnostics` returns as named records.
#' @export
yamaa_domain <- function(specification, environment = NULL, resolver = NULL) {
  args <- .domain_arguments(specification, environment)
  capabilities <- .yamaa_locked_host_capabilities()
  if (!is.null(resolver)) resolver <- resolver$session()
  host <- list(capabilities$verify, capabilities$resolve, .project_condition_details)
  handle <- if (is.null(resolver)) {
    .domain_reply(.native_domain_file(args[[1L]], args[[2L]], args[[3L]], host))
  } else {
    value <- .domain_reply(.native_domain_resolved(args[[1L]], args[[2L]], args[[3L]], host, resolver$dispatch))
    resolver$raise_pending()
    value
  }
  interrupted <- .domain_reply(.native_domain_interrupt(handle))
  if (!is.null(interrupted)) stop(interrupted)
  result <- new.env(parent = emptyenv())
  result$resource_provenance <- if (is.null(resolver)) list() else resolver$provenance
  makeActiveBinding("output", local({ h <- handle; function(value) {
    if (!missing(value)) stop("output is read only", call. = FALSE)
    .domain_reply(.native_domain_output(h))
  }}), result)
  makeActiveBinding("output_arrow", local({ h <- handle; function(value) {
    if (!missing(value)) stop("output_arrow is read only", call. = FALSE)
    .domain_arrow(h, 0L)
  }}), result)
  makeActiveBinding("verification_log_arrow", local({ h <- handle; function(value) {
    if (!missing(value)) stop("verification_log_arrow is read only", call. = FALSE)
    .domain_arrow(h, 1L)
  }}), result)
  makeActiveBinding("warning_log_arrow", local({ h <- handle; function(value) {
    if (!missing(value)) stop("warning_log_arrow is read only", call. = FALSE)
    .domain_arrow(h, 2L)
  }}), result)
  makeActiveBinding("issues", local({ h <- handle; function(value) {
    if (!missing(value)) stop("issues are read only", call. = FALSE)
    .domain_reply(.native_domain_issues(h))
  }}), result)
  makeActiveBinding("diagnostics", local({ h <- handle; function(value) {
    if (!missing(value)) stop("diagnostics are read only", call. = FALSE)
    .domain_reply(.native_domain_diagnostics(h))
  }}), result)
  makeActiveBinding("verification_log", local({ h <- handle; function(value) {
    if (!missing(value)) stop("verification_log is read only", call. = FALSE)
    .domain_reply(.native_domain_log(h, FALSE))
  }}), result)
  makeActiveBinding("warning_log", local({ h <- handle; function(value) {
    if (!missing(value)) stop("warning_log is read only", call. = FALSE)
    .domain_reply(.native_domain_log(h, TRUE))
  }}), result)
  result$save <- local({ h <- handle; function() {
    if (!is.null(resolver)) on.exit(resolver$raise_pending())
    reply <- .domain_reply(.native_domain_save(h))
    if (reply$failed) stop(structure(list(message = "cannot save a failed build", call = NULL),
      class = c("yamaa_domain_error", "error", "condition")))
    reply$saved
  }})
  lockEnvironment(result, bindings = TRUE)
  result
}

#' Check a specification without reading study data or running project code
#' @inheritParams yamaa_domain
#' @return A list containing the issues data frame and the diagnostics records,
#'   both built natively from the same findings.
#' @export
yamaa_check <- function(specification, environment = NULL, resolver = NULL) {
  args <- .domain_arguments(specification, environment)
  if (is.null(resolver)) {
    reply <- .domain_reply(.native_check_file(args[[1L]], args[[2L]], args[[3L]]))
  } else {
    resolver <- resolver$session()
    reply <- .domain_reply(.native_check_resolved(args[[1L]], args[[2L]], args[[3L]], resolver$dispatch))
    resolver$raise_pending()
  }
  list(issues = reply$issues, diagnostics = reply$diagnostics)
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
