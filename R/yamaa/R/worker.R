# The worker the standalone yamaa command line runs its R project functions
# through: Rscript -e 'yamaa:::.yamaa_worker()'.
#
# The binary runs on no interpreter, so a run that calls a project function
# starts this worker in the R whose library the project's lock names, and
# talks to it over standard input and output, one request line and one reply
# line at a time. The worker verifies the lock, resolves each callable and
# runs each call with the helpers of locked_environment.R, the ones the
# package's own run() uses, so a function behaves as it does there. The
# protocol is the one rust/crates/yamaa-cli/src/worker.rs describes:
# tab-separated fields, percent-encoded text, and typed values.
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

# The judgements below are the in-process port's, function_callback.rs of the
# R binding, value for value and reason for reason: only the class attribute
# makes a value classed, so a named number is plain, and a classed value
# must have exactly the class its type names.

# A length-one NA of a logical, integer or character vector, or a non-finite
# double, whatever its class: missing, which the engine then checks.
.worker_missing <- function(value) {
  bare <- unclass(value)
  if (length(bare) != 1L) return(FALSE)
  switch(typeof(bare), logical = , integer = , character = is.na(bare), double = !is.finite(bare), FALSE)
}

# One integer or double value, whatever its class, or NULL.
.worker_number <- function(value) {
  bare <- unclass(value)
  if (length(bare) == 1L && typeof(bare) %in% c("integer", "double")) as.numeric(bare)
}

# Name a refused value in a result's reason.
.worker_describe <- function(value) {
  class <- oldClass(value)
  if (!is.null(class)) return(sprintf("an object of class %s", encodeString(class[[1L]], quote = "\"")))
  kind <- switch(typeof(value), logical = "a logical vector", integer = "an integer vector",
    double = "a double vector", character = "a character vector", complex = "a complex vector",
    raw = "a raw vector", list = "a list", closure = , builtin = , special = return("a function"),
    environment = return("an environment"), return("an unsupported R object"))
  sprintf("%s of length %d", kind, length(value))
}

# Show a refused number in a reason as Rust shows a double: its shortest
# round-trip digits, in plain decimal, or with an exponent from 1e21.
.worker_shown <- function(value) {
  digits <- 1L
  while (digits < 17L && as.numeric(sprintf("%.*g", digits, value)) != value) digits <- digits + 1L
  if (abs(value) < 1e21) return(format(value, digits = digits, scientific = FALSE))
  sub("e\\+?(-?)0*([0-9])", "e\\1\\2", sprintf("%.*e", digits - 1L, value))
}

# NULL when a POSIXct's seconds are the clock times it shows, its tzone "UTC"
# or "GMT"; otherwise the zone, described for the refusal (#4882).
.worker_zone <- function(value) {
  zone <- attr(value, "tzone", exact = TRUE)
  if (is.null(zone)) return("the session's time zone")
  if (!is.character(zone) || length(zone) != 1L || is.na(zone) || identical(Encoding(zone), "bytes") ||
      !validUTF8(enc2utf8(zone))) return("an unreadable tzone")
  if (zone %in% c("UTC", "GMT")) return(NULL)
  if (!nzchar(zone)) return("the session's time zone")
  sprintf("tzone %s", encodeString(enc2utf8(zone), quote = "\""))
}

# The days and the seconds from 1970-01-01 of the civil years 1 to 9999.
.worker_days <- c(-719162, 2932896)
.worker_seconds <- c(-719162 * 86400, 2932896 * 86400 + 86399)

# One result as the protocol spells it, converted by the declared column type
# as the in-process port converts it: NULL, a length-one NA and a non-finite
# double are missing; anything else must be one value of the declared type,
# or the result is rejected with the reason.
.worker_encode_value <- function(value, returns) {
  if (is.null(value) || .worker_missing(value)) return("-")
  class <- oldClass(value)
  reject <- function(...) stop(.worker_rejected(sprintf(...)))
  # One number of exactly the class `expected`, NULL for none, or a refusal.
  number <- function(expected, article, what) {
    found <- .worker_number(value)
    if (is.null(found) || !identical(class, expected)) {
      reject("%s %s result must be %s, not %s", article, returns, what, .worker_describe(value))
    }
    found
  }
  whole <- function(found, what) {
    if (found != trunc(found)) reject("a %s result must be a whole number of %s, not %s", returns, what, .worker_shown(found))
  }
  within <- function(found, range, what) {
    if (found < range[[1L]] || found > range[[2L]]) reject("a %s result must lie within %s, not %s %s",
      returns, if (returns == "time") "one day" else "the years 1 to 9999", .worker_shown(found), what)
  }
  switch(returns,
    str = {
      if (typeof(value) != "character" || length(value) != 1L || !is.null(class)) {
        reject("a str result must be one character string, not %s", .worker_describe(value))
      }
      if (identical(Encoding(value), "bytes")) reject("byte-marked R text has no declared Unicode encoding")
      text <- enc2utf8(as.vector(value))
      if (!validUTF8(text)) reject("R text is not valid UTF-8")
      paste0("s:", .worker_encode(text))
    },
    int = {
      found <- number(NULL, "an", "one number")
      if (found != trunc(found) || abs(found) > 2^53) {
        reject("an int result must be a whole number within 2^53 in magnitude, not %s", .worker_shown(found))
      }
      sprintf("i:%.0f", found)
    },
    float = paste0("f:", sprintf("%.17g", number(NULL, "a", "one number"))),
    date = {
      days <- number("Date", "a", "one Date")
      whole(days, "days")
      within(days, .worker_days, "days from 1970-01-01")
      day <- as.POSIXlt(as.Date(days, origin = "1970-01-01"), tz = "UTC")
      sprintf("d:%04d-%02d-%02d", day$year + 1900L, day$mon + 1L, day$mday)
    },
    datetime = {
      seconds <- number(c("POSIXct", "POSIXt"), "a", "one POSIXct")
      zone <- .worker_zone(value)
      if (!is.null(zone)) reject("a datetime result must be a POSIXct with tzone UTC or GMT, not %s", zone)
      whole(seconds, "seconds")
      within(seconds, .worker_seconds, "seconds from 1970-01-01T00:00:00")
      second <- as.POSIXlt(seconds, origin = "1970-01-01", tz = "UTC")
      sprintf("t:%04d-%02d-%02dT%02d:%02d:%02d", second$year + 1900L, second$mon + 1L, second$mday,
        second$hour, second$min, as.integer(second$sec))
    },
    time = {
      units <- attr(value, "units", exact = TRUE)
      seconds <- if (identical(class, c("hms", "difftime")) && is.character(units) && length(units) == 1L &&
        identical(as.vector(units), "secs")) .worker_number(value)
      if (is.null(seconds)) reject("a time result must be one hms of seconds, not %s", .worker_describe(value))
      whole(seconds, "seconds")
      within(seconds, c(0, 86399), "seconds from midnight")
      sprintf("h:%02d:%02d:%02d", seconds %/% 3600, seconds %/% 60 %% 60, seconds %% 60)
    },
    reject("unsupported declared result type"))
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
      .worker_encode_value(do.call(bound$target, values, quote = TRUE), bound$returns)
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
