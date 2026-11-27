# The worker the standalone yamaa command line runs its R project functions
# through: Rscript -e 'yamaa:::.yamaa_worker()'.
#
# The binary runs on no interpreter, so a run that calls a project function
# starts this worker in the R whose library the project's lock names, and
# talks to it over standard input and output, one request line and one reply
# line at a time. The worker verifies the lock, resolves each callable and
# runs each call with the helpers of locked_environment.R, the ones the
# package's own run() uses, and converts each result with the package's
# compiled conversion, the one run() uses, so a function behaves as it does
# there. The protocol is the one rust/crates/yamaa-cli/src/worker.rs
# describes: tab-separated fields, percent-encoded text, and typed values.
#
# Standard output carries the protocol, so while the worker answers a request
# R's standard output is diverted to standard error, which is the
# terminal's: what a project function prints reaches it, as its messages and
# warnings do, and corrupts nothing.

# Percent-encode text: a byte of A-Za-z0-9-._~ as itself, any other as % and
# two uppercase hexadecimal digits.
.worker_encode <- function(text) {
  if (grepl("^[A-Za-z0-9._~-]*$", text)) return(text)
  bytes <- as.integer(charToRaw(enc2utf8(text)))
  keep <- bytes %in% c(45L, 46L, 95L, 126L, 48:57, 65:90, 97:122)
  pieces <- sprintf("%%%02X", bytes)
  pieces[keep] <- rawToChar(as.raw(bytes[keep]), multiple = TRUE)
  paste(pieces, collapse = "")
}

.worker_decode <- function(text) {
  decoded <- utils::URLdecode(text)
  Encoding(decoded) <- "UTF-8"
  decoded
}

# The fields of one protocol line, keeping empty ones: strsplit() drops a
# trailing empty field, so the text is split with one more separator.
.worker_fields <- function(text, separator) strsplit(paste0(text, separator), separator, fixed = TRUE)[[1L]]

# The name and the value of a `name=value` field, split at its first `=`.
.worker_pair <- function(pair) {
  split <- regexpr("=", pair, fixed = TRUE)
  if (split < 1L) stop(sprintf("the protocol field %s names no value", pair), call. = FALSE)
  list(name = .worker_decode(substr(pair, 1L, split - 1L)), value = substr(pair, split + 1L, nchar(pair)))
}

# The plain R value of one argument, as the in-process port gives it: NA for
# missing, doubles for numbers, a Date, a POSIXct in UTC, and an hms of
# seconds since midnight. A value R cannot hold exactly, such as an int
# beyond 2^53 in magnitude, never arrives: the binary refuses the call
# first, by the facade's judgement the in-process port applies too.
.worker_decode_value <- function(field) {
  if (identical(field, "-")) return(NA)
  kind <- substr(field, 1L, 1L)
  text <- substr(field, 3L, nchar(field))
  switch(kind,
    s = .worker_decode(text),
    i = as.numeric(text),
    f = switch(text, inf = Inf, "-inf" = -Inf, nan = NaN, as.numeric(text)),
    b = identical(text, "true"),
    d = as.Date(text, format = "%Y-%m-%d"),
    t = as.POSIXct(text, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
    h = {
      parts <- as.numeric(strsplit(text, ":", fixed = TRUE)[[1L]])
      structure(sum(parts * c(3600, 60, 1)), units = "secs", class = c("hms", "difftime"))
    },
    stop(sprintf("the protocol spells no value %s", field), call. = FALSE))
}

.worker_rejected <- function(reason) {
  structure(class = c("yamaa_worker_rejected", "condition"), list(message = reason, call = NULL))
}

# One result as the protocol spells it, converted by the declared column type
# with the compiled package's conversion, the one the in-process port uses
# (worker_result in function_callback.rs of the R binding, #5100): NULL, a
# length-one NA and a non-finite double are missing; anything else must be
# one value of the declared type, or the result is rejected with the reason.
.worker_result <- function(value, returns) {
  converted <- .reply(.native_worker_result(value, charToRaw(returns)))
  if (!is.null(converted$rejected)) stop(.worker_rejected(converted$rejected))
  converted$value
}

# The fields of the ok reply to one request, or a condition.
.worker_handle <- function(fields, handles) {
  request <- fields[[1L]]
  arguments <- fields[-1L]
  switch(request,
    info = .worker_encode(.core_version()),
    verify = {
      count <- as.integer(arguments[[1L]])
      calls <- vapply(arguments[seq_len(count) + 1L], .worker_decode, "")
      locked <- list()
      for (field in arguments[-seq_len(count + 1L)]) {
        pair <- .worker_pair(field)
        locked[[pair$name]] <- .worker_decode(pair$value)
      }
      findings <- .yamaa_verify_locked_versions(locked, calls)
      vapply(findings, function(finding) paste(
        .worker_encode(finding$package), .worker_encode(finding$reason),
        if (is.null(finding$expected)) "-" else .worker_encode(finding$expected),
        if (is.null(finding$actual)) "-" else .worker_encode(finding$actual), sep = "|"), "")
    },
    bind = {
      call <- .worker_decode(arguments[[1L]])
      returns <- arguments[[2L]]
      parameters <- vapply(arguments[-(1:2)], .worker_decode, "")
      target <- .yamaa_resolve_locked_function(call, parameters)
      handle <- length(handles$bound) + 1L
      handles$bound[[handle]] <- list(target = target, returns = returns)
      as.character(handle)
    },
    invoke = {
      bound <- handles$bound[[as.integer(arguments[[1L]])]]
      values <- list()
      for (field in arguments[-1L]) {
        pair <- .worker_pair(field)
        values[[pair$name]] <- .worker_decode_value(pair$value)
      }
      .worker_result(do.call(bound$target, values, quote = TRUE), bound$returns)
    },
    exit = character(),
    stop(sprintf("the protocol has no request %s", request), call. = FALSE))
}

# Evaluate `code` with R's standard output diverted to standard error, and
# end every diversion begun meanwhile, a project function's own included,
# however it ends.
.worker_diverted <- function(code) {
  depth <- sink.number()
  sink(stderr())
  on.exit(while (sink.number() > depth) sink())
  code
}

.yamaa_worker <- function(input = file("stdin", "r"), output = stdout()) {
  on.exit(close(input))
  handles <- new.env(parent = emptyenv())
  handles$bound <- list()
  read_line <- function() {
    line <- readLines(input, n = 1L, warn = FALSE)
    if (!length(line)) stop("the command line ended", call. = FALSE)
    line
  }
  repeat {
    line <- tryCatch(read_line(), error = function(condition) NULL)
    if (is.null(line)) break
    fields <- .worker_fields(line, "\t")
    reply <- tryCatch(
      c("ok", .worker_diverted(.worker_handle(fields, handles))),
      yamaa_worker_rejected = function(condition) c("rejected", .worker_encode(conditionMessage(condition)), "-"),
      interrupt = function(condition) "interrupt",
      error = function(condition) c("error", .worker_encode(class(condition)[[1L]]),
        .worker_encode(conditionMessage(condition))))
    cat(paste(reply, collapse = "\t"), "\n", sep = "", file = output)
    flush(output)
    if (identical(fields[[1L]], "exit")) break
  }
  invisible(NULL)
}
