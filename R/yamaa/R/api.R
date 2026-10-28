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
  error <- reply$error
  if (!is.null(error)) {
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

# A path argument (#4091) is the text of a character string: its names and
# class, such as an fs::path() or glue::glue() value carries, are dropped,
# and a leading `~` expands as base R's file functions expand it. Only a path
# expands; a name, a section or a TAR member name is text, which .text()
# takes as written.
.path <- function(value, what) {
  if (is.character(value)) value <- path.expand(as.character(unname(value)))
  .text(value, what)
}

.optional_path <- function(value, what) if (is.null(value)) NULL else .path(value, what)

.paths <- function(value, what) {
  if (!is.character(value) || length(value) == 0L || anyNA(value)) {
    stop(sprintf("`%s` must be a non-empty character vector without missing values", what),
      call. = FALSE)
  }
  lapply(path.expand(as.character(unname(value))), .utf8_raw)
}

.version <- function() charToRaw(as.character(getRversion()))

.capabilities <- function() {
  capabilities <- .yamaa_locked_host_capabilities()
  list(capabilities$verify, capabilities$resolve, .project_condition_details)
}

# Locked bindings refuse assignment, so a binding only reads.
.read_only <- function(object, name, get) makeActiveBinding(name, function() get(), object)

# A result's memo (#4071). Reading a table converts all of it from Rust, so a
# table binding keeps the data frame it read and answers a later read with
# it. One memo serves a result and each member run made from it, which read
# one native handle. A method of the result can change what the handle
# answers (a refused save() adds its issues), so each method call starts the
# memo again, however the call ends. The memo records the package load that
# made it: an object restored from saveRDS, an `.RData` file or another R
# process carries a copy, which is not this load's, so its reads reach the
# native handle, which refuses as expired.
.loaded <- new.env(parent = emptyenv())

.memo <- function() {
  memo <- new.env(parent = emptyenv())
  memo$loaded <- .loaded
  memo$generation <- 0L
  memo
}

# A table binding of a result: the data frame `get` returns is kept until a
# method of the result runs. Any other value is read again on each read: a
# failed run's NULL, and a streamed run's output, a fresh stream each time.
.kept <- function(object, name, memo, get) {
  kept <- NULL
  generation <- NA_integer_
  makeActiveBinding(name, function() {
    if (!identical(memo$loaded, .loaded)) return(get())
    if (identical(generation, memo$generation)) return(kept)
    value <- get()
    if (is.data.frame(value)) {
      kept <<- value
      generation <<- memo$generation
    }
    value
  }, object)
}

# `value`, the reply of a result's method, after which the memo starts again.
.changing <- function(memo, value) {
  on.exit(memo$generation <- memo$generation + 1L)
  value
}

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
