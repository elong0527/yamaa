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

.reply <- function(reply) {
  interrupted <- reply$interrupt
  if (!is.null(interrupted)) .signal_again(interrupted)
  error <- reply$error
  if (!is.null(error)) {
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

# D46: a native call stops at the first interrupt it is asked about, or that a
# project function raised, publishes nothing and returns that condition, which
# is signaled again here, the original object. Handlers established for the
# interrupt take it; without one, R returns to the top level, as an interrupt
# does. An error R signaled instead when asked, such as an elapsed time limit,
# is raised again.
.signal_again <- function(condition) {
  if (inherits(condition, "interrupt")) {
    signalCondition(condition)
    invokeRestart("abort")
  }
  stop(condition)
}

# Locked bindings refuse assignment, so a binding only reads.
.read_only <- function(object, name, get) makeActiveBinding(name, function() get(), object)

.object <- function(object, class) {
  lockEnvironment(object, bindings = TRUE)
  class(object) <- class
  object
}
