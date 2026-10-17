#' Resolve a specification into versioned inspection facts
#' @param specification One original domain specification path.
#' @param environment Optional study environment path.
#' @return An owned JSON character scalar of class yamaa_resolved_specification.
#'   Use as.character() to export or an optional JSON reader to inspect fields.
#' @details Named dictionary references are preserved in the normalized model.
#'   Locations and provenance are facts, not file access or execution capabilities.
#'   This function prepares metadata without invoking project callbacks.
#' @export
yamaa_resolve_specification <- function(specification, environment = NULL) {
  args <- .domain_arguments(specification, environment)
  reply <- .domain_reply(.native_resolve_specification_file(args[[1L]], args[[2L]]))
  if (is.null(reply$json)) {
    stop(structure(list(message = "cannot resolve specification; inspect issues", call = NULL,
      issues = reply$issues, diagnostics = reply$diagnostics),
      class = c("yamaa_resolved_specification_error", "error", "condition")))
  }
  structure(reply$json, class = "yamaa_resolved_specification")
}

#' Read a resolved specification for inspection
#' @param json One JSON character scalar in the versioned resolved format.
#' @return An owned JSON character scalar of class yamaa_resolved_specification.
#' @details This validates the format and version in the shared engine. It never
#'   opens resources, resolves paths, compiles a program or invokes callbacks.
#' @export
yamaa_read_resolved_specification <- function(json) {
  value <- .domain_reply(.native_read_resolved_specification(.domain_path_bytes(json)))
  structure(value, class = "yamaa_resolved_specification")
}

#' @export
as.character.yamaa_resolved_specification <- function(x, ...) unclass(x)
