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
# Standard output carries the protocol, so a project function must not write
# to it; its messages and warnings reach standard error, which is the
# terminal's.

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

# The plain R value of one argument, as the in-process port gives it: NA for
# missing, doubles for numbers, a Date, and a POSIXct in UTC.
.worker_decode_value <- function(field) {
  if (identical(field, "-")) return(NA)
  kind <- substr(field, 1L, 1L)
  text <- substr(field, 3L, nchar(field))
  switch(kind,
    s = .worker_decode(text),
    i = {
      value <- as.numeric(text)
      if (abs(value) > 2^53) stop("an int beyond 2^53 in magnitude has no exact R value", call. = FALSE)
      value
    },
    f = switch(text, inf = Inf, "-inf" = -Inf, nan = NaN, as.numeric(text)),
    b = identical(text, "true"),
    d = as.Date(text, format = "%Y-%m-%d"),
    t = as.POSIXct(text, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
    stop(sprintf("the protocol spells no value %s", field), call. = FALSE))
}

.worker_rejected <- function(reason) {
  structure(class = c("yamaa_worker_rejected", "condition"), list(message = reason, call = NULL))
}

.worker_number <- function(value) {
  (is.numeric(value) || is.integer(value)) && length(value) == 1L && is.null(attributes(value))
}

.worker_describe <- function(value) {
  if (is.null(attributes(value))) sprintf("a %s of length %d", typeof(value), length(value))
  else sprintf("a %s", paste(class(value), collapse = "/"))
}

# One result as the protocol spells it, converted by the declared column type
# as the in-process port converts it: NULL, a length-one NA and a non-finite
# double are missing; anything else must be one value of the declared type,
# or the result is rejected with the reason.
.worker_encode_value <- function(value, returns) {
  if (is.null(value) || (is.atomic(value) && length(value) == 1L && is.na(value))) return("-")
  if (.worker_number(value) && !is.finite(value)) return("-")
  reject <- function(expected) {
    stop(.worker_rejected(sprintf("%s result must be %s, not %s", returns, expected, .worker_describe(value))))
  }
  switch(returns,
    str = {
      if (!is.character(value) || length(value) != 1L || !is.null(attributes(value))) reject("one character string")
      paste0("s:", .worker_encode(value))
    },
    int = {
      if (!.worker_number(value)) reject("one number")
      if (value != trunc(value) || abs(value) > 2^53) reject("a whole number within 2^53 in magnitude")
      sprintf("i:%.0f", value)
    },
    float = {
      if (!.worker_number(value)) reject("one number")
      paste0("f:", sprintf("%.17g", value))
    },
    date = {
      if (!inherits(value, "Date") || length(value) != 1L) reject("one Date")
      days <- unclass(value)
      if (!is.finite(days) || days != trunc(days)) reject("a Date of whole days")
      paste0("d:", format(value, "%Y-%m-%d"))
    },
    datetime = {
      if (!inherits(value, "POSIXct") || length(value) != 1L) reject("one POSIXct")
      seconds <- unclass(value)
      if (!is.finite(seconds) || seconds != trunc(seconds)) reject("a POSIXct of whole seconds")
      paste0("t:", format(value, "%Y-%m-%dT%H:%M:%S", tz = "UTC"))
    },
    reject("a value of a declared type"))
}

# The fields of the ok reply to one request, or a condition.
.worker_handle <- function(fields, handles, read_line) {
  request <- fields[[1L]]
  arguments <- fields[-1L]
  switch(request,
    info = .worker_encode(.core_version()),
    verify = {
      count <- as.integer(arguments[[1L]])
      calls <- vapply(arguments[seq_len(count) + 1L], .worker_decode, "")
      locked <- list()
      for (pair in arguments[-seq_len(count + 1L)]) {
        parts <- strsplit(pair, "=", fixed = TRUE)[[1L]]
        locked[[.worker_decode(parts[[1L]])]] <- .worker_decode(parts[[2L]])
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
      for (pair in arguments[-1L]) {
        split <- regexpr("=", pair, fixed = TRUE)
        name <- .worker_decode(substr(pair, 1L, split - 1L))
        values[[name]] <- .worker_decode_value(substr(pair, split + 1L, nchar(pair)))
      }
      .worker_encode_value(do.call(bound$target, values, quote = TRUE), bound$returns)
    },
    batch = {
      bound <- handles$bound[[as.integer(arguments[[1L]])]]
      rows <- as.integer(arguments[[2L]])
      count <- as.integer(arguments[[3L]])
      columns <- list()
      for (index in seq_len(count)) {
        line <- strsplit(read_line(), "\t", fixed = TRUE)[[1L]]
        columns[[.worker_decode(line[[1L]])]] <- lapply(line[-1L], .worker_decode_value)
      }
      vapply(seq_len(rows), function(row) {
        values <- lapply(columns, function(column) column[[row]])
        .worker_encode_value(do.call(bound$target, values, quote = TRUE), bound$returns)
      }, "")
    },
    exit = character(),
    stop(sprintf("the protocol has no request %s", request), call. = FALSE))
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
    fields <- strsplit(line, "\t", fixed = TRUE)[[1L]]
    reply <- tryCatch(
      c("ok", .worker_handle(fields, handles, read_line)),
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
