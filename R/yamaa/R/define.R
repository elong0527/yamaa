# Public conversion only. Shared Rust owns composition, rendering and publication.
#' Generate submission documents through the shared Rust engine
#' @param environment One environment YAML path.
#' @param standard One of sdtm, adam or send.
#' @param created Optional whole-second civil datetime; omission uses current UTC.
#' @return An owned result with issues, diagnostics and a save method.
#' @export
yamaa_define <- function(environment, standard, created = NULL) {
  handle <- .domain_reply(.native_define_file(.domain_path_bytes(environment),
    .domain_path_bytes(standard), if (is.null(created)) NULL else .domain_path_bytes(created),
    charToRaw(.core_version())))
  result <- new.env(parent = emptyenv())
  makeActiveBinding("issues", local({ h <- handle; function(value) {
    if (!missing(value)) stop("issues are read only", call. = FALSE)
    .domain_reply(.native_define_issues(h))
  }}), result)
  makeActiveBinding("diagnostics", local({ h <- handle; function(value) {
    if (!missing(value)) stop("diagnostics are read only", call. = FALSE)
    .domain_reply(.native_define_diagnostics(h))
  }}), result)
  result$save <- local({ h <- handle; function() {
    reply <- .domain_reply(.native_define_save(h))
    if (reply$failed) stop(structure(list(message = reply$cause, call = NULL,
      issues = .domain_reply(.native_define_issues(h)),
      diagnostics = .domain_reply(.native_define_diagnostics(h))),
      class = c("yamaa_define_error", "error", "condition")))
    reply$saved
  }})
  lockEnvironment(result, bindings = TRUE)
  result
}

# Private installed evidence reads the original held byte owner.
.define_artifacts <- function(result) {
  handle <- get("h", envir = environment(result$save), inherits = FALSE)
  .domain_reply(.native_define_artifacts(handle))
}
