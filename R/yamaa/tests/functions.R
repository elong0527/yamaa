library(yamaa)

# JSON construction for independent test inputs; no installed JSON dependency.
jquote <- function(x) {
  code <- utf8ToInt(enc2utf8(x))
  chars <- vapply(code, function(n) {
    if (n == 34L) return('\\"')
    if (n == 92L) return('\\\\')
    if (n < 32L) return(sprintf("\\u%04x", n))
    intToUtf8(n)
  }, "")
  paste0('"', paste(chars, collapse = ""), '"')
}
split_first <- function(x, sep = ":") {
  pos <- regexpr(sep, x, fixed = TRUE)[1L]
  c(substr(x, 1L, pos - 1L), substring(x, pos + nchar(sep)))
}
bits <- function(x) paste(format(writeBin(x, raw(), size = 8L, endian = "big")), collapse = "")
wire <- function(token, float_bits = FALSE) {
  if (token == "missing") return('{"missing":null}')
  part <- split_first(token)
  kind <- part[1L]; value <- part[2L]
  if (kind == "float" && !float_bits) value <- bits(as.double(value))
  if (kind %in% c("date", "datetime")) {
    value <- paste0('{"text":', jquote(value), ',"precision":"',
                    if (kind == "date") "day" else "second", '"}')
  } else if (kind != "bool") value <- jquote(value)
  paste0('{"', kind, '":', value, '}')
}
request <- function(parameters = character(), arguments = character(), returns = "int", nullable = FALSE) {
  paste0('{"protocol":"function/1","identity":{"name":"example",',
         '"contract_version":"1","implementation_version":"2","call":"artifact.example"},',
         '"parameters":[', paste(parameters, collapse = ","), '],"arguments":[',
         paste(arguments, collapse = ","), '],"returns":', jquote(returns),
         ',"may_return_missing":', if (nullable) "true" else "false", '}')
}
param <- function(name = "x", kind = "int", missing = FALSE, default = "required", host = "host_x") {
  paste0('{"name":', jquote(name), ',"host_name":', jquote(host), ',"type":', jquote(kind),
         ',"accepts_missing":', if (missing) "true" else "false", ',"presence":',
         if (default == "required") '{"required":null}' else paste0('{"optional":', wire(default), '}'), '}')
}
argument <- function(name = "x", value = '{"int":"1"}') {
  paste0('{"name":', jquote(name), ',"value":', value, '}')
}
value_outcome <- function(value) paste0('{"protocol":"function/1","outcome":{"status":"value","value":', value, '}}')
contains <- function(text, fragment) stopifnot(grepl(fragment, text, fixed = TRUE))
condition <- function(text, code, requirement) {
  contains(text, '"status":"condition"')
  contains(text, '"phase":"derivation"')
  contains(text, paste0('"condition":"', code, '"'))
  contains(text, paste0('"requirement":"', requirement, '"'))
  contains(text, '"applicable_handler":null')
  for (entry in c('"function":"example"', '"contract_version":"1"', '"implementation_version":"2"')) contains(text, entry)
}
# Platform strftime implementations do not all pad %Y for years below 1000.
# Observe civil fields explicitly so the independent trace spelling is portable.
temporal_text <- function(x, datetime = FALSE) {
  fields <- as.POSIXlt(x, tz = "UTC")
  date <- sprintf("%04d-%02d-%02d", fields$year + 1900L, fields$mon + 1L, fields$mday)
  if (!datetime) return(date)
  paste0(date, sprintf("T%02d:%02d:%02d", fields$hour, fields$min, as.integer(fields$sec)))
}
observe <- function(x) {
  if (identical(x, NA)) return("missing")
  if (identical(class(x), "yamaa_int64")) return(paste0("int:", as.character(x)))
  if (identical(class(x), "yamaa_utf8")) return(paste0("str:", as.character(x)))
  if (identical(class(x), "Date")) return(paste0("date:", temporal_text(x)))
  if (identical(class(x), c("POSIXct", "POSIXt"))) {
    stopifnot(identical(attr(x, "tzone"), "UTC"))
    return(paste0("datetime:", temporal_text(x, TRUE)))
  }
  if (is.double(x)) return(paste0("float:", bits(x)))
  if (is.logical(x)) return(paste0("bool:", tolower(as.character(x))))
  stop("unexpected host representation")
}
host_value <- function(token) {
  if (token == "missing") return(NA)
  part <- split_first(token)
  switch(part[1L], int = int64(part[2L]), str = part[2L],
         float = as.double(part[2L]), bool = identical(part[2L], "true"),
         date = as.Date(part[2L]),
         datetime = as.POSIXct(part[2L], format = "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
         stop("unexpected test value"))
}

# Replay the existing shared independent truth with real R callbacks and trace.
cases <- read.delim(system.file("function_invocation.tsv", package = "yamaa"),
                    sep = "\t", quote = "", comment.char = "", colClasses = "character",
                    fileEncoding = "UTF-8", check.names = FALSE)
stopifnot(nrow(cases) == 42L)
for (i in seq_len(nrow(cases))) {
  case <- cases[i, ]
  parameters <- character(); arguments <- character()
  if (case$parameters != "-") for (p in strsplit(case$parameters, ";", fixed = TRUE)[[1L]]) {
    p <- strsplit(p, "/", fixed = TRUE)[[1L]]
    parameters <- c(parameters, param(p[1L], p[2L], p[3L] == "true", p[4L], p[5L]))
  }
  if (case$arguments != "-") for (a in strsplit(case$arguments, ";", fixed = TRUE)[[1L]]) {
    a <- split_first(a, "=")
    arguments <- c(arguments, argument(a[1L], wire(a[2L])))
  }
  calls <- 0L; trace <- "-"
  callback <- function(...) {
    calls <<- calls + 1L
    args <- list(...)
    trace <<- if (length(args)) paste(paste0(names(args), "=", vapply(args, observe, "")), collapse = ";") else "called"
    token <- case$callback
    if (token == "raise") stop(structure(list(message = "boom", call = NULL),
                                          class = c("ValueError", "error", "condition")))
    if (startsWith(token, "echo:")) return(args[[substring(token, 6L)]])
    if (token == "invalid:list") return(list(1L))
    if (token == "invalid:bigint") return(structure("9223372036854775808", class = "yamaa_int64"))
    host_value(token)
  }
  result <- invoke_function(request(parameters, arguments, case$returns, case$may_missing == "true"), callback)
  if (!identical(trace, case$trace)) stop(sprintf(
    "%s: callback trace %s differs from %s", case$id, jquote(trace), jquote(case$trace)))
  stopifnot(identical(calls, if (case$trace == "-") 0L else 1L))
  expected <- case$expected
  if (!startsWith(expected, "error:")) {
    stopifnot(identical(result, value_outcome(wire(expected, float_bits = TRUE))))
  } else {
    fields <- strsplit(expected, ":", fixed = TRUE)[[1L]]
    if (fields[2L] %in% c("missing", "unknown", "argument")) {
      condition(result, "invalid_function_argument", "REQ-0700")
      if (fields[2L] == "unknown") contains(result, '"unknown":["a","z"]') else {
        contains(result, paste0('"parameter":', jquote(fields[3L])))
        if (fields[2L] == "argument") {
          contains(result, paste0('"expected":', jquote(fields[4L])))
          contains(result, paste0('"actual":', jquote(fields[5L])))
        } else contains(result, '"reason":"a required argument was not supplied"')
      }
    } else if (fields[2L] == "raised") {
      condition(result, "function_call_failed", "REQ-0701")
      contains(result, '"host_error":"ValueError"'); contains(result, '"host_message":"boom"')
      contains(result, '"call":"artifact.example"')
    } else {
      condition(result, "invalid_function_result", "REQ-0702")
      if (fields[2L] == "result") {
        contains(result, paste0('"expected":', jquote(fields[3L])))
        contains(result, paste0('"actual":', jquote(fields[4L])))
      } else if (fields[2L] == "boolean") contains(result, '"reason":"a binding returned a Boolean"') else if (fields[2L] == "undeclared-missing") {
        contains(result, '"reason":"an invoked binding returned an undeclared missing"')
      } else {
        # R has no built-in arbitrary integer: its forged out-of-range carrier is
        # rejected during representation admission, before logical result checks.
        contains(result, '"reason":"a binding returned a value of no scalar type"')
        contains(result, paste0('"returned":"', if (fields[3L] == "list") "list" else "character", '"'))
      }
    }
  }
}
cat("42 shared installed R callback contracts passed\n")

# Full scalar fidelity, useful arithmetic, ownership, nesting and caller process.
int_req <- request(param(), argument(value = wire("int:9007199254740993")))
saved <- NULL; pid <- Sys.getpid(); effects <- 0L
result <- invoke_function(int_req, function(host_x) {
  stopifnot(identical(Sys.getpid(), pid))
  saved <<- host_x
  invisible(gc())
  effects <<- effects + 1L
  host_x + 1L
})
stopifnot(identical(result, value_outcome(wire("int:9007199254740994"))), effects == 1L)
rm(int_req); invisible(gc())
stopifnot(identical(as.character(saved), "9007199254740993"))
saved[] <- "7"
stopifnot(identical(result, value_outcome(wire("int:9007199254740994"))))
base_req <- request()
stopifnot(identical(invoke_function(base_req, function() {
  stopifnot(identical(invoke_function(base_req, function() 2L), value_outcome(wire("int:2"))))
  int64("7")
}), value_outcome(wire("int:7"))))

nul_wire <- '{"str":"a\\u0000b"}'
text_req <- request(param(kind = "str"), argument(value = nul_wire), "str")
held <- NULL
result <- invoke_function(text_req, function(host_x) {
  stopifnot(identical(class(host_x), "yamaa_utf8"), length(host_x) == 1L,
            identical(utf8_bytes(host_x), as.raw(c(97L, 0L, 98L))))
  held <<- host_x
  host_x
})
stopifnot(identical(result, value_outcome(nul_wire)))
rm(text_req); invisible(gc())
stopifnot(identical(utf8_bytes(held), as.raw(c(97L, 0L, 98L))))
held[1L] <- as.raw(120L)
stopifnot(identical(result, value_outcome(nul_wire)))
for (text in c("", "\u96ea\U0001f980", "e\u0301", "\u00e9")) {
  result <- invoke_function(request(returns = "str"), function() text)
  stopifnot(identical(result, value_outcome(wire(paste0("str:", text)))))
}
latin1 <- rawToChar(as.raw(233L)); Encoding(latin1) <- "latin1"
stopifnot(identical(invoke_function(request(returns = "str"), function() latin1),
                    value_outcome(wire("str:\u00e9"))))

# Exact temporal fields and explicit UTC are retained; collected precision ends
# at host argument encoding. No machine timezone participates in epoch conversion.
for (entry in list(
  list(kind = "date", text = "0001-01-01", epoch = -719162, precision = "year"),
  list(kind = "date", text = "9999-12-31", epoch = 2932896, precision = "month"),
  list(kind = "datetime", text = "0001-01-01T00:00:00", epoch = -62135596800, precision = "day"),
  list(kind = "datetime", text = "9999-12-31T23:59:59", epoch = 253402300799, precision = "day")
)) {
  value <- paste0('{"', entry$kind, '":{"text":', jquote(entry$text),
                  ',"precision":', jquote(entry$precision), '}}')
  req <- request(param(kind = entry$kind), argument(value = value), entry$kind)
  result <- invoke_function(req, function(host_x) {
    numeric <- unclass(host_x); attributes(numeric) <- NULL
    stopifnot(identical(numeric, entry$epoch))
    if (entry$kind == "datetime") stopifnot(identical(attr(host_x, "tzone"), "UTC"))
    host_x
  })
  stopifnot(identical(result, value_outcome(wire(paste0(entry$kind, ":", entry$text)))))
}
for (value in list(
  structure(-719163, class = "Date"), structure(2932897, class = "Date"),
  structure(0.5, class = "Date"), structure("0", class = "Date"),
  structure(-62135596801, class = c("POSIXct", "POSIXt"), tzone = "UTC"),
  structure(253402300800, class = c("POSIXct", "POSIXt"), tzone = "UTC"),
  structure(0.5, class = c("POSIXct", "POSIXt"), tzone = "UTC"),
  structure(0, class = c("POSIXct", "POSIXt")),
  structure(0, class = c("POSIXct", "POSIXt"), tzone = "America/New_York"),
  structure(0, class = c("subclass", "Date")), as.POSIXlt(0, origin = "1970-01-01", tz = "UTC")
)) condition(invoke_function(base_req, function() value), "invalid_function_result", "REQ-0702")
for (value in list(NA, NA_integer_, NA_real_, NA_character_, NaN, Inf, -Inf,
                   structure(NA_real_, class = "Date"),
                   structure(Inf, class = c("POSIXct", "POSIXt"), tzone = "UTC"))) {
  stopifnot(identical(invoke_function(request(nullable = TRUE), function() value), value_outcome(wire("missing"))))
  condition(invoke_function(base_req, function() value), "invalid_function_result", "REQ-0702")
}

# Invalid return representations are not callback-raised conditions.
bad_text <- rawToChar(as.raw(255L)); Encoding(bad_text) <- "UTF-8"
byte_text <- latin1; Encoding(byte_text) <- "bytes"
for (value in list(NULL, integer(), 1:2, list(1L), 1+1i, raw(1), factor("x"),
                   new.env(), bad_text, byte_text, setNames(1L, "x"),
                   structure(1L, class = "unknown"), structure("1", class = c("subclass", "yamaa_int64")),
                   structure(as.raw(255L), class = "yamaa_utf8"))) {
  effects <- 0L
  result <- invoke_function(base_req, function() { effects <<- effects + 1L; value })
  condition(result, "invalid_function_result", "REQ-0702")
  stopifnot(effects == 1L)
}

# Primary conditions preserve identity, secondary-rendering failures do not replace
# them, and interrupts escape only after Rust returns. No side effect is replayed.
render_calls <- 0L
conditionMessage.BadDetail <- function(c) {
  render_calls <<- render_calls + 1L
  # base::stop renders once before signaling; fail only inside our handler.
  if (render_calls == 1L) c$message else stop("secondary rendering failure")
}
problem <- structure(list(message = "original", call = NULL), class = c("BadDetail", "error", "condition"))
effects <- 0L
result <- invoke_function(base_req, function() { effects <<- effects + 1L; stop(problem) })
condition(result, "function_call_failed", "REQ-0701")
contains(result, '"host_error":"BadDetail"')
contains(result, '"host_message":"unavailable R condition message"')
stopifnot(effects == 1L)
render_calls <- 0L
conditionMessage.BadDetail <- function(c) {
  render_calls <<- render_calls + 1L
  if (render_calls == 1L) c$message else bad_text
}
result <- invoke_function(base_req, function() stop(problem))
contains(result, '"host_error":"BadDetail"')
contains(result, '"host_message":"unavailable R condition message"')
rm(conditionMessage.BadDetail)
long_message <- strrep("\u96ea", 3000L)
problem <- structure(list(message = long_message, call = NULL), class = c("WideMessage", "error", "condition"))
result <- invoke_function(base_req, function() stop(problem))
condition(result, "function_call_failed", "REQ-0701")
contains(result, '"host_details_truncated":true')
contains(result, '"host_error":"WideMessage"')
contains(result, paste0('"host_message":', jquote(strrep("\u96ea", 2730L))))
original_interrupt <- structure(list(message = "cancel", token = new.env()), class = c("interrupt", "condition"))
effects <- 0L
caught <- tryCatch(invoke_function(base_req, function() {
  effects <<- effects + 1L
  stop(original_interrupt)
}), interrupt = identity)
stopifnot(identical(caught, original_interrupt), effects == 1L)
stopifnot(identical(invoke_function(base_req, function() 9L), value_outcome(wire("int:9"))))

# Bound names, signature and request shape before any callback effects.
for (name in c("...", "..1", "..123", ".", ".1x", "_x", "for", "NA", "NA_integer_", "\u00e9", "a-b")) {
  effects <- 0L
  result <- tryCatch(invoke_function(request(param(host = name), argument()), function(...) {
    effects <<- effects + 1L; 1L
  }), error = identity)
  stopifnot(inherits(result, "error"), effects == 0L,
            identical(conditionMessage(result), "invalid function host argument name"))
}
for (name in c(".x", "a.b", "x_1", "..x", "..")) {
  result <- invoke_function(request(param(host = name), argument()), function(...) {
    stopifnot(identical(names(list(...)), name)); 1L
  })
  stopifnot(identical(result, value_outcome(wire("int:1"))))
}
for (req in c("{", sub('function/1', 'function/2', base_req, fixed = TRUE),
              request(c(param(), param()), argument()),
              request(vapply(seq_len(257L), function(i) param(paste0("x", i), host = paste0("x", i)), "")),
              strrep("x", 1048577L))) {
  effects <- 0L
  failure <- tryCatch(invoke_function(req, function(...) { effects <<- effects + 1L; 1L }), error = identity)
  stopifnot(inherits(failure, "error"), effects == 0L)
}
stopifnot(inherits(try(invoke_function(base_req, "sum"), silent = TRUE), "try-error"))
# Oversized valid return is a resource error after exactly one callback effect.
effects <- 0L
failure <- tryCatch(invoke_function(request(returns = "str"), function() {
  effects <<- effects + 1L
  strrep("x", 1048577L)
}), error = identity)
stopifnot(inherits(failure, "error"), effects == 1L,
          identical(conditionMessage(failure), "function output exceeds resource limit"))
stopifnot(identical(invoke_function(base_req, function() 1L), value_outcome(wire("int:1"))))

# Direct internal dispatch cannot bypass raw payload validation or crash recovery.
symbol <- get("wrap__invoke_function", envir = asNamespace("yamaa"))
for (dispatch in list(function(...) list(), function(...) list(99L, NULL, NULL),
                      function(...) list(0L, 1L, "unchecked text"))) {
  stopifnot(identical(.Call(symbol, charToRaw(base_req), dispatch)$error, "internal function transport failure"))
}
result <- .Call(symbol, charToRaw(request(returns = "str")), function(...) list(0L, 3L, as.raw(255L)))
stopifnot(is.null(result$error))
condition(result$value, "invalid_function_result", "REQ-0702")
stopifnot(identical(.Call(symbol, as.raw(255L), function(...) stop("must not run"))$error, "invalid function request"))
stopifnot(identical(invoke_function(base_req, function() 1L), value_outcome(wire("int:1"))))
cat("Installed R callback ownership, temporal, condition and resource contracts passed\n")
