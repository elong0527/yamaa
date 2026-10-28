# Handwritten support for the public R API, whose exported functions, object
# constructors and S3 methods public-bindings.R generates from
# rust/interface/bindings.yaml. Every method makes one native call; behavior,
# validation and messages live in the shared Rust engine, so R and Python
# report the same results. An object's first `run()` compiles its
# specifications and keeps them in a native holder; a later `run()` reuses them
# while its paths still name the files they were compiled from and every one is
# unchanged, and compiles again otherwise. The holder is made on the first
# `run()`, and made again when the object was restored from `saveRDS`, an
# `.RData` file or another R process.

# A reply's value, or its refusal signalled. An interrupt the call kept is
# signalled again as R's own: an interrupt handler takes it, and unhandled it
# runs the `interrupt` option's function and returns to the top level, as
# Ctrl+C does. stop() would end it as an error, which an error handler takes
# (#4070).
.reply <- function(reply) {
  error <- reply$error
  if (!is.null(error)) {
    if (inherits(error$condition, "interrupt")) {
      signalCondition(error$condition)
      hook <- getOption("interrupt")
      if (is.function(hook)) hook()
      invokeRestart("abort")
    }
    if (!is.null(error$condition)) stop(error$condition)
    stop(structure(
      list(message = error$message, call = NULL, issues = error$issues, paths = error$paths),
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

.texts <- function(value, what) {
  if (!is.character(value) || length(value) == 0L || anyNA(value)) {
    stop(sprintf("`%s` must be a non-empty character vector without missing values", what),
      call. = FALSE)
  }
  lapply(unname(value), .utf8_raw)
}

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

# A progress callback (#3762): `NULL`, or the user's function wrapped so it
# replies with a closed envelope and no R error crosses Rust: list(0L, NULL)
# when it returned, list(1L, condition) when it signalled an error, which
# fails the result, and list(3L, condition) for an interrupt, which cancels
# the call. Rust gives it one named list per progress event.
.progress <- function(progress, what) {
  if (is.null(progress)) return(NULL)
  if (!is.function(progress)) {
    stop(sprintf("`%s` must be a function or NULL", what), call. = FALSE)
  }
  force(progress)
  function(event) tryCatch({
    progress(event)
    list(0L, NULL)
  }, interrupt = function(condition) list(3L, condition),
  error = function(condition) list(1L, condition))
}
