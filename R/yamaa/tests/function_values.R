# Project functions exchange plain R values with the engine: missing is NA,
# str is character, int and float are double, bool is logical, date is Date
# and datetime is POSIXct in UTC. A value R cannot hold exactly is refused,
# never changed, and a result converts by its declared type. Inputs and truth
# are authored here, independently of the engine.
library(yamaa)
namespace <- asNamespace("yamaa")
capabilities <- get(".yamaa_locked_host_capabilities", namespace)()
with_resolve <- function(resolve, code) {
  name <- ".yamaa_locked_host_capabilities"
  original <- get(name, namespace)
  replace <- function(value) {
    unlockBinding(name, namespace)
    assign(name, value, namespace)
    lockBinding(name, namespace)
  }
  on.exit(replace(original))
  replace(function() list(verify = capabilities$verify, resolve = resolve))
  force(code)
}
# The installed binding, wrapped to record the plain arguments of every call
# and to reply with `value` instead when `when` selects the argument `x`.
wrapped <- function(record = function(arguments) NULL, when = function(x) FALSE, value = NULL) {
  function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      record(arguments)
      if (when(arguments$x)) list(0L, value) else invoke(arguments)
    })
  }
}
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
reasons <- function(issues) vapply(issues$context, function(context)
  jsonlite::fromJSON(context, simplifyVector = FALSE)$reason, "", USE.NAMES = FALSE)
refused <- function(result, condition, requirement, reason) {
  stopifnot(is.null(result$output), nrow(result$issues) == 1L,
    identical(result$issues$phase, "derivation"),
    identical(result$issues$condition, condition),
    identical(result$issues$requirement, requirement),
    identical(reasons(result$issues), reason))
  failure <- tryCatch(result$save(), error = identity)
  stopifnot(inherits(failure, "yamaa_error"),
    identical(conditionMessage(failure), "cannot save a failed run; see issues"),
    identical(failure$issues, result$issues))
}
# The project's one specification, run with the project's functions.
run <- function() yamaa_project("project.yaml")$spec("TEST")$run()

work <- tempfile("function-values-"); dir.create(work)
previous <- setwd(work)
writeLines(c("schema_version: '1.0'", "environments: {r: environment.yaml}",
  "adam: {specs: [spec.yaml]}"), "project.yaml")
identity_function <- function(name, type, cases, extra = character()) c(
  paste0("  ", name, ":"), "    function: yamaabenchmarks::project_value",
  paste0("    description: Return the ", type, " argument unchanged."), extra,
  paste0("    params: [{name: x, type: ", type,
    if (length(extra)) ", accepts_missing: true" else "", "}]"),
  paste0("    returns: ", type), "    tests:", paste0("      - ", cases))
missing_case <- "{id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}"
ratio <- readLines(system.file("project-original", "schema-functions", "r", "environment.yaml",
  package = "yamaa", mustWork = TRUE))
writeLines(c("schema_version: '1.0'", "functions:",
  identity_function("int_value", "int", c(
    "{id: normal, covers: [normal], args: {x: 7}, result: 7}",
    "{id: boundary, covers: [boundary], args: {x: 9007199254740992}, result: 9007199254740992}",
    missing_case)),
  identity_function("float_value", "float", c(
    "{id: normal, covers: [normal, numeric-comparison], args: {x: 2.5}, result: 2.5}",
    "{id: boundary, covers: [boundary, numeric-comparison], args: {x: 0.0}, result: 0.0}",
    missing_case)),
  identity_function("text_value", "str", c(
    "{id: normal, covers: [normal], args: {x: \"\\u00e9t\\u00e9\"}, result: \"\\u00e9t\\u00e9\"}",
    "{id: boundary, covers: [boundary], args: {x: ''}, result: ''}",
    missing_case)),
  identity_function("date_value", "date", c(
    "{id: normal, covers: [normal], args: {x: {date: '2024-02-29'}}, result: {date: '2024-02-29'}}",
    "{id: boundary, covers: [boundary], args: {x: {date: '1969-12-31'}}, result: {date: '1969-12-31'}}",
    missing_case)),
  identity_function("datetime_value", "datetime", c(
    "{id: normal, covers: [normal], args: {x: {datetime: '2024-02-29T12:30:45'}}, result: {datetime: '2024-02-29T12:30:45'}}",
    "{id: boundary, covers: [boundary], args: {x: {datetime: '1969-12-31T23:59:59'}}, result: {datetime: '1969-12-31T23:59:59'}}",
    missing_case)),
  identity_function("maybe_int", "int", c(
    "{id: normal, covers: [normal], args: {x: 1}, result: 1}",
    "{id: boundary, covers: [boundary], args: {x: 0}, result: 0}",
    "{id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}"),
    "    may_return_missing: true"),
  ratio[seq(which(ratio == "functions:") + 1L, length(ratio))]), "environment.yaml")

# One build calls every function, so activation runs every declared case.
writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: input.csv, types: {ID: int, I: int, F: float, S: str, D: date, T: datetime, M: int}}}",
  "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
  "  - {name: I, type: int, derivation: {function: {name: int_value, args: {x: SRC.I}}}}",
  "  - {name: F, type: float, derivation: {function: {name: float_value, args: {x: SRC.F}}}}",
  "  - {name: S, type: str, derivation: {function: {name: text_value, args: {x: SRC.S}}}}",
  "  - {name: D, type: date, derivation: {function: {name: date_value, args: {x: SRC.D}}}}",
  "  - {name: T, type: datetime, derivation: {function: {name: datetime_value, args: {x: SRC.T}}}}",
  "  - {name: M, type: int, derivation: {function: {name: maybe_int, args: {x: SRC.M}}}}",
  "  - name: P", "    type: float", "    derivation:", "      function:", "        name: project_ratio",
  "        args: {numerator: SRC.F, denominator: 4.0, adjust: {literal: null}, as_percent: true}",
  "output: {path: output.csv, columns: [ID, I, F, S, D, T, M, P]}"), "spec.yaml")
writeBin(charToRaw(enc2utf8(paste0("ID,I,F,S,D,T,M\n",
  "1,7,2.5,\u00e9t\u00e9,2024-02-29,2024-02-29T12:30:45,5\n",
  "2,9007199254740992,-0.5,x,0001-01-01,9999-12-31T23:59:59,\n",
  "3,-9007199254740992,,,,,-9007199254740992\n"))), "input.csv")
seen <- list()
record <- function(arguments) seen[[length(seen) + 1L]] <<- arguments
received <- function(arguments) any(vapply(seen, identical, logical(1), arguments))
for (attempt in 1:2) {
  seen <- list()
  built <- with_resolve(wrapped(record), run())
  stopifnot(nrow(built$issues) == 0L,
    identical(built$output$ID, c(1, 2, 3)),
    identical(built$output$I, c(7, 2^53, -2^53)),
    identical(built$output$F, c(2.5, -0.5, NA)),
    identical(built$output$S, c("\u00e9t\u00e9", "x", NA)),
    identical(built$output$D, as.Date(c("2024-02-29", "0001-01-01", NA))),
    identical(built$output$T, as.POSIXct(c("2024-02-29 12:30:45", "9999-12-31 23:59:59", NA), tz = "UTC")),
    identical(built$output$M, c(5, NA, -2^53)),
    identical(built$output$P, c(62.5, -12.5, NA)))
  # Each argument arrives as its one plain R value: int as double, accepted
  # missing as logical NA, bool as logical and an omitted default by value.
  stopifnot(received(list(x = 7)), received(list(x = 2^53)), received(list(x = -2^53)),
    received(list(x = 2.5)), received(list(x = "\u00e9t\u00e9")), received(list(x = "")),
    received(list(x = as.Date("2024-02-29"))), received(list(x = as.Date("0001-01-01"))),
    received(list(x = as.POSIXct("2024-02-29 12:30:45", tz = "UTC"))),
    received(list(x = as.POSIXct("9999-12-31 23:59:59", tz = "UTC"))),
    received(list(x = 5)), received(list(x = NA)),
    received(list(numerator = 2.5, denominator = 4, decimals = 2, adjust = NA, as_percent = TRUE)),
    received(list(numerator = 1, denominator = 2, decimals = 2, adjust = 0, as_percent = FALSE)),
    !received(list(x = NULL)))
  text <- Filter(function(arguments) identical(arguments, list(x = "\u00e9t\u00e9")), seen)[[1L]]$x
  stopifnot(identical(Encoding(text), "UTF-8"),
    identical(attributes(Filter(function(arguments) inherits(arguments$x, "POSIXct"), seen)[[1L]]$x),
      list(class = c("POSIXct", "POSIXt"), tzone = "UTC")))
  stopifnot(is.null(built$save()), identical(rawfile("output.csv"), charToRaw(enc2utf8(paste0(
    "ID,I,F,S,D,T,M,P\n",
    "1,7,2.5,\u00e9t\u00e9,2024-02-29,2024-02-29T12:30:45,5,62.5\n",
    "2,9007199254740992,-0.5,x,0001-01-01,9999-12-31T23:59:59,,-12.5\n",
    "3,-9007199254740992,,,,,-9007199254740992,\n")))))
  unlink("output.csv")
}
cat("plain R function arguments and results round trip every type and repeat\n")

# One function column over authored cells, each cell raw bytes or text.
column <- function(type, name, cells, resolve = capabilities$resolve) {
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    paste0("input: {SRC: {path: input.csv, types: {ID: int, X: ", type, "}}}"),
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    paste0("  - {name: X, type: ", type, ", derivation: {function: {name: ", name,
      ", args: {x: SRC.X}}}}"),
    "output: {path: output.csv, columns: [ID, X]}"), "spec.yaml")
  rows <- lapply(seq_along(cells), function(index) c(charToRaw(paste0(index, ",")),
    if (is.raw(cells[[index]])) cells[[index]] else charToRaw(enc2utf8(cells[[index]])),
    charToRaw("\n")))
  writeBin(c(charToRaw("ID,X\n"), unlist(rows)), "input.csv")
  with_resolve(resolve, run())
}

# Text beyond one MiB crosses both ways whole and saves exact bytes.
large <- paste0(strrep("\u00e9", 524289L), "complete")
long <- column("str", "text_value", list(large))
stopifnot(nrow(long$issues) == 0L, identical(long$output$X, large), is.null(long$save()),
  identical(rawfile("output.csv"), charToRaw(enc2utf8(paste0("ID,X\n1,", large, "\n")))))
unlink("output.csv")

# An argument R cannot hold exactly fails the call before project code runs.
for (value in c("9007199254740993", "-9007199254740993", "9223372036854775807")) {
  seen <- list()
  refused(column("int", "int_value", list("7", value), wrapped(record)),
    "function_call_failed", "REQ-0701", paste0("argument x: integer ", value,
      " is beyond 2^53 and cannot be held exactly by an R double"))
  stopifnot(received(list(x = 7)), !any(vapply(seen, function(arguments)
    abs(arguments$x) > 2^53, logical(1))))
}
refused(column("str", "text_value", list(as.raw(c(0x61, 0x00, 0x62)))),
  "function_call_failed", "REQ-0701",
  "argument x: text containing NUL cannot be held by an R character vector")
cat("integers beyond 2^53 and text holding NUL are refused before the call\n")

# A result converts by the declared type; anything else is an invalid result.
invalid <- list(
  list("int", "int_value", "42", c(1, 2), "an int result must be one number, not a double vector of length 2"),
  list("int", "int_value", "42", TRUE, "an int result must be one number, not a logical vector of length 1"),
  list("int", "int_value", "42", "42", "an int result must be one number, not a character vector of length 1"),
  list("int", "int_value", "42", factor("a"), "an int result must be one number, not an object of class \"factor\""),
  list("int", "int_value", "42", 2.5, "an int result must be a whole number within 2^53 in magnitude, not 2.5"),
  list("int", "int_value", "42", 2^53 + 2, "an int result must be a whole number within 2^53 in magnitude, not 9007199254740994"),
  list("float", "float_value", "4.5", list(4.5), "a float result must be one number, not a list of length 1"),
  list("float", "float_value", "4.5", structure(4.5, class = "difftime", units = "days"),
    "a float result must be one number, not an object of class \"difftime\""),
  list("str", "text_value", "trigger", 1, "a str result must be one character string, not a double vector of length 1"),
  list("str", "text_value", "trigger", c("a", "b"), "a str result must be one character string, not a character vector of length 2"),
  list("date", "date_value", "2000-01-01", "2000-01-01", "a date result must be one Date, not a character vector of length 1"),
  list("date", "date_value", "2000-01-01", structure(0.5, class = "Date"), "a date result must be a whole number of days, not 0.5"),
  list("date", "date_value", "2000-01-01", structure(3e6, class = "Date"),
    "a date result must lie within the years 1 to 9999, not 3000000 days from 1970-01-01"),
  list("datetime", "datetime_value", "2000-01-01T00:00:00", as.Date("2000-01-01"),
    "a datetime result must be one POSIXct, not an object of class \"Date\""),
  list("datetime", "datetime_value", "2000-01-01T00:00:00", as.POSIXct("2000-01-01", tz = "America/New_York"),
    "a datetime result must be a POSIXct with tzone UTC, not tzone \"America/New_York\""),
  list("datetime", "datetime_value", "2000-01-01T00:00:00", structure(0, class = c("POSIXct", "POSIXt")),
    "a datetime result must be a POSIXct with tzone UTC, not one without a tzone"),
  list("datetime", "datetime_value", "2000-01-01T00:00:00", .POSIXct(0.5, tz = "UTC"),
    "a datetime result must be a whole number of seconds, not 0.5"))
trigger <- function(type, cell) switch(type,
  int = function(x) identical(x, 42), float = function(x) identical(x, 4.5),
  str = function(x) identical(x, cell), date = function(x) identical(x, as.Date(cell)),
  datetime = function(x) identical(x, as.POSIXct(cell, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC")))
for (case in invalid) {
  result <- column(case[[1L]], case[[2L]], list(case[[3L]]),
    wrapped(when = trigger(case[[1L]], case[[3L]]), value = case[[4L]]))
  refused(result, "invalid_function_result", "REQ-0702", case[[5L]])
}
refused(column("int", "int_value", list("42"), wrapped(when = trigger("int"), value = NULL)),
  "invalid_function_result", "REQ-0702", "an invoked binding returned an undeclared missing")

# Integer storage, names, Date storage and every NA spelling are plain values.
accepted <- list(
  list("int", "int_value", "42", 7L, 7), list("float", "float_value", "4.5", 9L, 9),
  list("float", "float_value", "4.5", c(named = 1.5), 1.5),
  list("date", "date_value", "2000-01-01", structure(19782L, class = "Date"), as.Date("2024-02-29")),
  list("datetime", "datetime_value", "2000-01-01T00:00:00", as.POSIXct("2024-02-29 12:30:45", tz = "UTC"),
    as.POSIXct("2024-02-29 12:30:45", tz = "UTC")))
for (value in list(NULL, NA, NA_integer_, NA_real_, NA_character_, NaN, Inf, -Inf)) {
  accepted[[length(accepted) + 1L]] <- list("int", "maybe_int", "42", value, NA_real_)
}
for (case in accepted) {
  result <- column(case[[1L]], case[[2L]], list(case[[3L]]),
    wrapped(when = trigger(case[[1L]], case[[3L]]), value = case[[4L]]))
  stopifnot(nrow(result$issues) == 0L, identical(result$output$X, case[[5L]]))
}
cat("results convert by the declared type and every other value is refused\n")
setwd(previous)
unlink(work, recursive = TRUE)
