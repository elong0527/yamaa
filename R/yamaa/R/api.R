# Public R API: lazy specification and project objects over native operations.
# Every method makes one native call; behavior, validation and messages live
# in the shared Rust engine, so R and Python report the same results.

.reply <- function(reply) {
  error <- reply$error
  if (!is.null(error)) {
    stop(structure(
      list(message = error$message, call = NULL, issues = error$issues),
      class = c("yamaa_error", "error", "condition")
    ))
  }
  reply$value
}

.text <- function(value, what) {
  if (!is.character(value) || length(value) != 1L || is.na(value)) {
    stop(sprintf("`%s` must be one non-missing character string", what), call. = FALSE)
  }
  .utf8_raw(value)
}

.optional_text <- function(value, what) if (is.null(value)) NULL else .text(value, what)

.version <- function() charToRaw(as.character(getRversion()))

.capabilities <- function() {
  capabilities <- .yamaa_locked_host_capabilities()
  list(capabilities$verify, capabilities$resolve, .project_condition_details)
}

# Locked bindings refuse assignment, so a binding only reads.
.read_only <- function(object, name, get) makeActiveBinding(name, function() get(), object)

.object <- function(object, class) {
  lockEnvironment(object, bindings = TRUE)
  class(object) <- class
  object
}

.check <- function(issues) {
  object <- new.env(parent = emptyenv())
  object$issues <- issues
  .object(object, "yamaa_check")
}

.run <- function(handle) {
  interrupted <- .reply(.native_run_interrupt(handle))
  if (!is.null(interrupted)) stop(interrupted)
  object <- new.env(parent = emptyenv())
  object$.handle <- handle
  .read_only(object, "output", function() .reply(.native_run_output(handle)))
  .read_only(object, "issues", function() .reply(.native_run_issues(handle)))
  .read_only(object, "report", function() .reply(.native_run_report(handle)))
  object$save <- function() invisible(.reply(.native_run_save(handle)))
  .object(object, "yamaa_run")
}

# Private installed-run evidence for qualification suites, as Python's
# `run._native.observations()`.
.run_observations <- function(run, include_measurements = FALSE) {
  handle <- get(".handle", envir = run, inherits = FALSE)
  .reply(.native_run_observations(handle, include_measurements))
}

.spec <- function(path, project) {
  object <- new.env(parent = emptyenv())
  object$check <- function() .check(.reply(.native_spec_check(path, project, .version())))
  object$resolve <- function() .reply(.native_spec_resolve(path, project, .version()))
  object$run <- function() {
    .run(.reply(.native_spec_run(path, project, .version(), .capabilities())))
  }
  .object(object, "yamaa_spec")
}

.member <- function(handle, name) {
  key <- .text(name, "name")
  if (!name %in% .reply(.native_project_run_names(handle))) {
    .reply(.native_project_run_member_issues(handle, key)) # raises the engine's refusal
  }
  object <- new.env(parent = emptyenv())
  .read_only(object, "output", function() .reply(.native_project_run_output(handle, key)))
  .read_only(object, "issues", function() .reply(.native_project_run_member_issues(handle, key)))
  .read_only(object, "report", function() .reply(.native_project_run_member_report(handle, key)))
  .object(object, "yamaa_member_run")
}

.project_run <- function(handle) {
  interrupted <- .reply(.native_project_run_interrupt(handle))
  if (!is.null(interrupted)) stop(interrupted)
  object <- new.env(parent = emptyenv())
  object$.handle <- handle
  .read_only(object, "names", function() .reply(.native_project_run_names(handle)))
  .read_only(object, "issues", function() .reply(.native_project_run_issues(handle)))
  .read_only(object, "report", function() .reply(.native_project_run_report(handle)))
  object$save <- function() invisible(.reply(.native_project_run_save(handle)))
  .object(object, "yamaa_project_run")
}

#' @export
`[[.yamaa_project_run` <- function(x, i, ...) {
  .member(get(".handle", envir = x, inherits = FALSE), i)
}

.submission <- function(handle) {
  object <- new.env(parent = emptyenv())
  .read_only(object, "issues", function() .reply(.native_submission_issues(handle)))
  object$save <- function() invisible(.reply(.native_submission_save(handle)))
  .object(object, "yamaa_submission")
}

#' One specification; nothing is read until a method runs
#' @param path Path of a specification file.
#' @return An object with `$check()`, `$resolve()` and `$run()`.
#' @export
yamaa_spec <- function(path) .spec(.text(path, "path"), NULL)

#' A project file; nothing is read until a method runs
#' @param path Path of a `project.yaml`.
#' @return An object with `$spec(name)`, `$check()`, `$run(section)` and
#'   `$submission(section, created)`.
#' @export
yamaa_project <- function(path) {
  project <- .text(path, "path")
  object <- new.env(parent = emptyenv())
  object$spec <- function(name) {
    path <- .reply(.native_project_spec(project, .text(name, "name"), .version()))
    .spec(.text(path, "path"), project)
  }
  object$check <- function() .check(.reply(.native_project_check(project, .version())))
  object$run <- function(section = NULL) {
    .project_run(.reply(.native_project_run(
      project, .optional_text(section, "section"), .version(), .capabilities()
    )))
  }
  object$submission <- function(section = NULL, created = NULL) {
    .submission(.reply(.native_project_submission(
      project, .optional_text(section, "section"), .optional_text(created, "created"),
      .version()
    )))
  }
  .object(object, "yamaa_project")
}

#' Read the clinical items of an ODM XML file, or of one member of a TAR
#' @param source Path of an ODM XML file or TAR archive.
#' @param archive_member Optional name of the XML member to read from a TAR.
#' @param max_archive_members,max_expanded_bytes,max_xml_depth Parsing limits.
#' @return A data frame with one row per clinical item.
#' @export
yamaa_read_odm <- function(source, archive_member = NULL, max_archive_members = 10000,
                           max_expanded_bytes = 268435456, max_xml_depth = 64) {
  .reply(.native_read_odm(
    .text(source, "source"), .optional_text(archive_member, "archive_member"),
    max_archive_members, max_expanded_bytes, max_xml_depth
  ))
}
