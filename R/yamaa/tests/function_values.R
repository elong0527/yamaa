# Project functions exchange plain R values with the engine: missing is NA,
# str is character, int and float are double, bool is logical, date is Date
# and datetime is POSIXct in UTC. A value R cannot hold exactly is refused,
# never changed, and a result converts by its declared type. Inputs and truth
# are authored here, independently of the engine.
library(yamaa)
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
# jsonlite is a suggested package: where it is not installed, a refusal's
# reason, which its JSON context holds, is not compared, and the rest is.
has_jsonlite <- requireNamespace("jsonlite", quietly = TRUE)
if (!has_jsonlite) message("Skipped the refusal reasons of function_values.R: jsonlite, a suggested package, is not installed")
namespace <- asNamespace("yamaa")
capabilities <- get(".yamaa_locked_host_capabilities", namespace)()
with_resolve <- function(resolve, code) with_capabilities(capabilities$verify, resolve, code)
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
reasons <- function(issues) vapply(issues$context, function(context)
  jsonlite::fromJSON(context, simplifyVector = FALSE)$reason, "", USE.NAMES = FALSE)
# An issue's file as its printed line names it (P-ISSUE-1): relative to the
# working directory when the file is beneath it, and absolute otherwise.
# Beneath is a matter of directories, not spellings: the engine names the file
# canonically, while the working directory may be spelled another way, such
# as with Windows 8.3 short names. So each directory above the file is
# compared with the working directory as normalizePath() spells both.
printed_file <- function(file) {
  here <- normalizePath(getwd(), winslash = "/")
  parts <- strsplit(file, "/", fixed = TRUE)[[1L]]
  for (depth in rev(seq_len(length(parts) - 1L))) {
    # The first part is the root: "" for "/", or a drive such as "C:".
    directory <- paste(parts[seq_len(depth)], collapse = "/")
    if (depth == 1L) directory <- paste0(directory, "/")
    if (identical(normalizePath(directory, winslash = "/", mustWork = FALSE), here))
      return(paste(parts[-seq_len(depth)], collapse = "/"))
  }
  file
}
# The run of one refused call, named by its `reason`.
refused <- function(result, condition, rule, reason) {
  stopifnot_case(reason, "a refused call leaves no output" = is.null(result$output))
  expect_identical(as.list(result$issues[c("phase", "condition", "rule")]),
    list(phase = "derivation", condition = condition, rule = rule), paste(reason, "is the one issue"))
  if (has_jsonlite) expect_identical(reasons(result$issues), reason, "the issue's reason")
  failure <- tryCatch(result$save(), error = identity)
  # D39: the run names its dataset, then its one issue, which starts with its
  # location (P-ISSUE-1).
  file <- result$issues$file
  location <- if (is.na(file)) "" else sprintf("%s:%d:%d: ", printed_file(file),
    as.integer(result$issues$line), as.integer(result$issues$column))
  stopifnot_case(reason, "its save signals yamaa_error" = inherits(failure, "yamaa_error"))
  expect_identical(conditionMessage(failure), sprintf("cannot save TEST: %s%s", location, issue_line(result$issues)),
    paste(reason, "gives D39's text"))
  expect_identical(failure$issues, result$issues, paste(reason, "carries the run's issues"))
}
# The project's one specification, run with the project's functions.
run <- function() yamaa_project("project.yaml")$spec("TEST")$run()

# Read before the runs move into their own directory.
rounding <- normalizePath(file.path("project-base", "base-round", "functions", "rounded_value.yaml"))
work <- tempfile("function-values-"); dir.create(work)
previous <- setwd(work)
# One function file per function, each implemented in base R (D8).
dir.create("functions")
function_names <- c("int_value", "float_value", "text_value", "date_value", "datetime_value",
  "maybe_int", "rounded_value", "bool_value")
writeLines(c("yamaa: '1.0'", "type: project", "functions:",
  paste0("  - functions/", function_names, ".yaml"), "adam: {specs: [spec.yaml]}"), "project.yaml")
function_file <- function(name, lines) writeLines(c("yamaa: '1.0'", "type: function",
  paste0("name: ", name), lines), file.path("functions", paste0(name, ".yaml")))
identity_function <- function(name, type, cases, extra = character()) function_file(name, c(
  paste0("description: Return the ", type, " argument unchanged."), extra,
  paste0("params: [{name: x, type: ", type,
    if (length(extra)) ", accepts_missing: true" else "", "}]"),
  paste0("returns: ", type), "implementations: {r: base::identity}", "tests:",
  paste0("  - ", cases)))
missing_case <- "{id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}"
identity_function("int_value", "int", c(
  "{id: normal, covers: [normal], args: {x: 7}, result: 7}",
  "{id: boundary, covers: [boundary], args: {x: 9007199254740992}, result: 9007199254740992}",
  missing_case))
identity_function("float_value", "float", c(
  "{id: normal, covers: [normal, numeric-comparison], args: {x: 2.5}, result: 2.5}",
  "{id: boundary, covers: [boundary, numeric-comparison], args: {x: 0.0}, result: 0.0}",
  missing_case))
identity_function("text_value", "str", c(
  "{id: normal, covers: [normal], args: {x: \"\\u00e9t\\u00e9\"}, result: \"\\u00e9t\\u00e9\"}",
  "{id: boundary, covers: [boundary], args: {x: ''}, result: ''}",
  missing_case))
identity_function("date_value", "date", c(
  "{id: normal, covers: [normal], args: {x: '2024-02-29'}, result: '2024-02-29'}",
  "{id: boundary, covers: [boundary], args: {x: '1969-12-31'}, result: '1969-12-31'}",
  missing_case))
identity_function("datetime_value", "datetime", c(
  "{id: normal, covers: [normal], args: {x: '2024-02-29T12:30:45'}, result: '2024-02-29T12:30:45'}",
  "{id: boundary, covers: [boundary], args: {x: '1969-12-31T23:59:59'}, result: '1969-12-31T23:59:59'}",
  missing_case))
identity_function("maybe_int", "int", c(
  "{id: normal, covers: [normal], args: {x: 1}, result: 1}",
  "{id: boundary, covers: [boundary], args: {x: 0}, result: 0}",
  "{id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}"),
  "may_return_missing: true")
stopifnot("the rounding function is copied" = file.copy(rounding, "functions"))
function_file("bool_value", c(
  "description: Select an integer using a boolean argument.",
  "params:", "  - {name: test, type: bool}",
  "  - {name: yes, type: int, required: false, default: 1}",
  "  - {name: no, type: int, required: false, default: 0}",
  "returns: int", "implementations: {r: base::ifelse}", "tests:",
  "  - {id: true-value, covers: [normal, boundary, 'boolean-true:test', 'default:yes', 'default:no'], args: {test: true}, result: 1}",
  "  - {id: false-value, covers: ['boolean-false:test'], args: {test: false, yes: 1, no: 0}, result: 0}",
  "  - {id: missing-test, covers: ['short-circuit-missing:test'], args: {test: null, yes: 1, no: 0}, result: null}",
  "  - {id: missing-yes, covers: ['short-circuit-missing:yes'], args: {test: true, yes: null, no: 0}, result: null}",
  "  - {id: missing-no, covers: ['short-circuit-missing:no'], args: {test: false, yes: 1, no: null}, result: null}"))

# One build calls every function, so activation runs every declared case.
writeLines(c("yamaa: '1.0'", "type: spec", "model: ADaM", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: input.csv, types: {ID: int, I: int, F: float, S: str, D: date, T: datetime, M: int}}}",
  "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
  "  - {name: I, type: int, derivation: {function: {name: int_value, args: {x: SRC.I}}}}",
  "  - {name: F, type: float, derivation: {function: {name: float_value, args: {x: SRC.F}}}}",
  "  - {name: S, type: str, derivation: {function: {name: text_value, args: {x: SRC.S}}}}",
  "  - {name: D, type: date, derivation: {function: {name: date_value, args: {x: SRC.D}}}}",
  "  - {name: T, type: datetime, derivation: {function: {name: datetime_value, args: {x: SRC.T}}}}",
  "  - {name: M, type: int, derivation: {function: {name: maybe_int, args: {x: SRC.M}}}}",
  "  - name: P", "    type: float", "    derivation:", "      function:", "        name: rounded_value",
  "        args: {x: SRC.F}",
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
  label <- sprintf("round trip %d", attempt)
  stopifnot_case(label, "the build has no issues" = nrow(built$issues) == 0L)
  expect_identical(as.list(built$output), list(ID = c(1, 2, 3), I = c(7, 2^53, -2^53), F = c(2.5, -0.5, NA),
    S = c("\u00e9t\u00e9", "x", NA), D = as.Date(c("2024-02-29", "0001-01-01", NA)),
    T = as.POSIXct(c("2024-02-29 12:30:45", "9999-12-31 23:59:59", NA), tz = "UTC"),
    M = c(5, NA, -2^53), P = c(2.5, -0.5, NA)), paste(label, "converts every result by its type"))
  # Each argument arrives as its one plain R value: int as double, accepted
  # missing as logical NA and an omitted default by value.
  for (arguments in list(list(x = 7), list(x = 2^53), list(x = -2^53), list(x = 2.5), list(x = "\u00e9t\u00e9"),
    list(x = ""), list(x = as.Date("2024-02-29")), list(x = as.Date("0001-01-01")),
    list(x = as.POSIXct("2024-02-29 12:30:45", tz = "UTC")), list(x = as.POSIXct("9999-12-31 23:59:59", tz = "UTC")),
    list(x = 5), list(x = NA), list(x = 2.5, digits = 3), list(x = 2.375, digits = 3))) {
    if (!received(arguments)) fail_expectation(paste(label, "receives each argument as its plain R value"), seen, arguments)
  }
  stopifnot_case(label, "no call receives NULL" = !received(list(x = NULL)))
  text <- Filter(function(arguments) identical(arguments, list(x = "\u00e9t\u00e9")), seen)[[1L]]$x
  expect_identical(Encoding(text), "UTF-8", paste(label, "marks text UTF-8"))
  expect_identical(attributes(Filter(function(arguments) inherits(arguments$x, "POSIXct"), seen)[[1L]]$x),
    list(class = c("POSIXct", "POSIXt"), tzone = "UTC"), paste(label, "passes a datetime in UTC"))
  stopifnot_case(label, "the build saves" = returns_paths(built$save()))
  expect_identical(rawfile("output.csv"), charToRaw(enc2utf8(paste0(
    "ID,I,F,S,D,T,M,P\n",
    "1,7,2.5,\u00e9t\u00e9,2024-02-29,2024-02-29T12:30:45,5,2.5\n",
    "2,9007199254740992,-0.5,x,0001-01-01,9999-12-31T23:59:59,,-0.5\n",
    "3,-9007199254740992,,,,,-9007199254740992,\n"))), paste(label, "saves the exact CSV"))
  unlink("output.csv")
}
cat("plain R function arguments and results round trip every type and repeat\n")

# One function column over authored cells, each cell raw bytes or text.
column <- function(type, name, cells, resolve = capabilities$resolve,
                   parameter = "x", argument = "SRC.X") {
  writeLines(c("yamaa: '1.0'", "type: spec", "model: ADaM", "domain: TEST", "keys: [ID]",
    paste0("input: {SRC: {path: input.csv, types: {ID: int, X: ", type, "}}}"),
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    paste0("  - {name: X, type: ", type, ", derivation: {function: {name: ", name,
      ", args: {", parameter, ": ", argument, "}}}}"),
    "output: {path: output.csv, columns: [ID, X]}"), "spec.yaml")
  rows <- lapply(seq_along(cells), function(index) c(charToRaw(paste0(index, ",")),
    if (is.raw(cells[[index]])) cells[[index]] else charToRaw(enc2utf8(cells[[index]])),
    charToRaw("\n")))
  writeBin(c(charToRaw("ID,X\n"), unlist(rows)), "input.csv")
  with_resolve(resolve, run())
}

# Boolean arguments stay logical; missing short-circuits the call.
seen <- list()
for (value in c("true", "false", "null")) {
  logicals <- column("int", "bool_value", list("1"), wrapped(record),
    parameter = "test", argument = value)
  stopifnot_case(paste("test:", value), "the build has no issues" = nrow(logicals$issues) == 0L)
  expect_identical(logicals$output$X, switch(value, true = 1, false = 0, null = NA_real_), paste("test:", value))
}
stopifnot("TRUE arrives logical" = received(list(test = TRUE, yes = 1, no = 0)),
  "FALSE arrives logical" = received(list(test = FALSE, yes = 1, no = 0)),
  "a missing test short-circuits the call" = !received(list(test = NA, yes = 1, no = 0)))

# Text beyond one MiB crosses both ways whole and saves exact bytes.
large <- paste0(strrep("\u00e9", 524289L), "complete")
long <- column("str", "text_value", list(large))
stopifnot("the large text build has no issues" = nrow(long$issues) == 0L,
  "the callback returns the whole text" = identical(long$output$X, large),
  "the large text build saves" = returns_paths(long$save()),
  "the saved CSV holds the whole text" =
    identical(rawfile("output.csv"), charToRaw(enc2utf8(paste0("ID,X\n1,", large, "\n")))))
unlink("output.csv")

# An argument R cannot hold exactly fails the call before project code runs.
for (value in c("9007199254740993", "-9007199254740993", "9223372036854775807")) {
  seen <- list()
  refused(column("int", "int_value", list("7", value), wrapped(record)),
    "function_call_failed", "FUNC-8", paste0("argument x: integer ", value,
      " is beyond 2^53 and cannot be held exactly by an R double"))
  stopifnot_case(paste("argument", value), "the other row is called" = received(list(x = 7)),
    "no call receives the unheld integer" = !any(vapply(seen, function(arguments) abs(arguments$x) > 2^53, logical(1))))
}
refused(column("str", "text_value", list(as.raw(c(0x61, 0x00, 0x62)))),
  "function_call_failed", "FUNC-8",
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
  refused(result, "invalid_function_result", "FUNC-9", case[[5L]])
}
refused(column("int", "int_value", list("42"), wrapped(when = trigger("int"), value = NULL)),
  "invalid_function_result", "FUNC-9", "an invoked binding returned an undeclared missing")

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
  label <- sprintf("%s result %s", case[[2L]], paste(deparse(case[[4L]]), collapse = ""))
  stopifnot_case(label, "an accepted result has no issues" = nrow(result$issues) == 0L)
  expect_identical(result$output$X, case[[5L]], paste(label, "converts to its plain value"))
}
cat("results convert by the declared type and every other value is refused\n")
setwd(previous)
unlink(work, recursive = TRUE)
