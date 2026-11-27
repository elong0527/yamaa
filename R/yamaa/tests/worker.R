# Installed .yamaa_worker(): the worker the standalone yamaa command line
# runs its R project functions through (cli.worker of
# rust/interface/bindings.yaml). The Rust tests cover the protocol's codec,
# rust/crates/yamaa/src/host_internal/worker.rs, and the binary's side; this
# drives the worker as the binary does, over its standard input and output,
# and checks what it answers: the installed engine's version, the renv lock
# findings, a bound callable's results, and the failures it reports in the
# engine's own terms. Python reads the same seam.
library(yamaa)
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)

enc <- yamaa:::.worker_encode
dec <- yamaa:::.worker_decode

# The replies to `lines`, then `exit`, from a worker run as Rscript runs it;
# its standard error goes to the file `errors`, or nowhere.
worker <- function(lines, errors = FALSE) {
  input <- tempfile("yamaa-worker-in-")
  on.exit(unlink(input))
  writeLines(c(lines, "exit"), input)
  replies <- system2(file.path(R.home("bin"), "Rscript"),
    c("-e", shQuote("yamaa:::.yamaa_worker()")), stdin = input, stdout = TRUE, stderr = errors)
  stopifnot("the worker answers every request and the exit" = identical(replies[[length(replies)]], "ok"))
  replies[-length(replies)]
}

expect_identical(enc("base::signif"), "base%3A%3Asignif", "text percent-encodes every byte outside the unreserved set")
expect_identical(dec("a%20b%2F%C3%A9"), "a b/\u00e9", "percent-encoded UTF-8 decodes to the text")
expect_identical(enc("a b/\u00e9"), "a%20b%2F%C3%A9", "the encoding round-trips")
expect_identical(yamaa:::.worker_fields("verify\t1\t\t", "\t"), c("verify", "1", "", ""),
  "a protocol line keeps its trailing empty fields")
cat("the codec spells text as the protocol says\n")

# The worker converts a result with the compiled package's conversion, the
# in-process port's, and spells it as the protocol spells a value, by the
# protocol's codec, rust/crates/yamaa/src/host_internal/worker.rs, whose
# floats are the shortest decimals that read back as themselves.
convert <- yamaa:::.worker_result
for (case in list(
  list(NULL, "float", "-", "NULL is missing"),
  list(NA_real_, "float", "-", "NA is missing"),
  list(Inf, "float", "-", "a non-finite double is missing"),
  list(2.5, "float", "f:2.5", "a float spells as a round-trip decimal"),
  list(3, "int", "i:3", "a whole double is an int"),
  list(3L, "int", "i:3", "an integer is an int"),
  list("a b", "str", "s:a%20b", "a string is percent-encoded"),
  list(as.Date("2024-02-29"), "date", "d:2024-02-29", "a Date spells its day"),
  list(as.POSIXct("2024-02-29 23:59:58", tz = "UTC"), "datetime", "t:2024-02-29T23:59:58",
    "a POSIXct spells its second in UTC"),
  list(c(n = 3), "int", "i:3", "a named number is plain, as only a class makes a value classed"),
  list(factor(NA), "int", "-", "a length-one NA of any class is missing"),
  list(as.Date(Inf), "date", "-", "a non-finite double of any class is missing"),
  list(as.Date("0999-12-31"), "date", "d:0999-12-31", "a Date spells a four-digit year"),
  list(structure(0, class = c("POSIXct", "POSIXt"), tzone = "GMT"), "datetime", "t:1970-01-01T00:00:00",
    "a POSIXct in GMT spells its second"),
  list(structure(30600, class = c("hms", "difftime"), units = "secs"), "time", "h:08:30:00",
    "an hms spells its second of the day"))) {
  expect_identical(convert(case[[1L]], case[[2L]]), case[[3L]], case[[4L]])
}
# The reason of each rejected result, read from
# rust/tests/host-fixtures/r-function-results.json, the truth that
# function_values.R checks the in-process port against: both routes give it.
# jsonlite, a suggested package, reads it; where it is not installed, those
# cases are skipped.
reject_reason <- function(value, returns) tryCatch(convert(value, returns), yamaa_worker_rejected = conditionMessage)
if (requireNamespace("jsonlite", quietly = TRUE)) {
  source(file.path(host_fixture_root, "reader.R"), local = TRUE)
  for (case in read_r_function_results(host_fixture_root)) {
    expect_identical(reject_reason(fixture_r_value(case$value), case$returns), case$reason,
      paste("the", case$returns, "result", paste(deparse(case$value), collapse = "")))
  }
} else {
  message("Skipped the rejected results of worker.R: their truth is JSON, and jsonlite is not installed")
}
# A time result, which the in-process suite does not call, outside one day.
expect_identical(reject_reason(structure(86400, class = c("hms", "difftime"), units = "secs"), "time"),
  "a time result must lie within one day, not 86400 seconds from midnight", "an hms of one whole day is refused")
decode <- yamaa:::.worker_decode_value
expect_identical(decode("-"), NA, "a missing argument is NA")
expect_identical(decode("s:a%20b"), "a b", "a str argument decodes")
expect_identical(decode("i:3"), 3, "an int argument is a double")
expect_identical(decode("i:-9007199254740992"), -2^53, "an int of magnitude 2^53 is exact")
expect_identical(decode("f:inf"), Inf, "an infinite float decodes")
expect_identical(decode("b:true"), TRUE, "a bool argument is a logical")
expect_identical(decode("d:2024-02-29"), as.Date("2024-02-29"), "a date argument is a Date")
expect_identical(decode("t:2024-02-29T23:59:58"), as.POSIXct("2024-02-29 23:59:58", tz = "UTC"),
  "a datetime argument is a POSIXct in UTC")
cat("the codec converts values as the in-process port does\n")

replies <- worker("info")
expect_identical(replies, paste0("ok\t", enc(yamaa:::.core_version())), "the worker names the installed engine")
cat("the worker names the installed engine\n")

installed <- as.character(utils::packageVersion("yamaa"))
replies <- worker(paste("verify", "1", enc("base::signif"), paste0(enc("yamaa"), "=", enc("0.0.0")), sep = "\t"))
expect_identical(replies, paste0("ok\t", enc("yamaa"), "|", enc("version_mismatch"), "|", enc("0.0.0"), "|", enc(installed)),
  "the worker reports a locked yamaa version that differs from the installed one")
expect_identical(worker(paste("verify", "1", enc("base::signif"), paste0(enc("yamaa"), "=", enc(installed)), sep = "\t")),
  "ok", "a lock that matches the installation has no finding")
# An empty locked version is a field of its own: the lock check finds it
# invalid, with the empty text as the version expected and none installed.
expect_identical(worker(paste("verify", "1", enc("base::signif"), paste0(enc("yamaa"), "="), sep = "\t")),
  paste0("ok\t", enc("yamaa"), "|", enc("invalid_locked_version"), "||-"),
  "an empty locked version is reported as invalid")
cat("the worker reports the renv lock findings\n")

replies <- worker(c(
  paste("bind", enc("base::signif"), "float", "x", "digits", sep = "\t"),
  "invoke\t1\tx=f:2.375\tdigits=i:3",
  "invoke\t1\tx=-\tdigits=i:3",
  paste("bind", enc("base::no_such_function"), "str", "x", sep = "\t"),
  paste("bind", enc("base::paste"), "str", "x", sep = "\t"),
  paste("bind", enc("base::nchar"), "int", "x", sep = "\t"),
  "invoke\t2\tx=s:abc",
  paste("bind", enc("base::list"), "str", sep = "\t"),
  "invoke\t3",
  paste("bind", enc("base::stop"), "str", sep = "\t")))
expect_identical(replies[1:2], c("ok\t1", "ok\tf:2.38"),
  "a bound function answers each call")
expect_identical(replies[[3L]], "ok\t-", "a missing argument gives a missing result")
stopifnot("an unknown callable is reported as an error" = startsWith(replies[[4L]], "error\t"))
expect_identical(replies[[5L]], paste0("error\tsimpleError\t", enc("installed callable signature differs from params")),
  "a callable whose signature differs from the parameters is not bound")
cat("the worker binds installed callables and reports binding failures\n")

# nchar's formals differ from one parameter, so it is not bound; list() and
# stop() take only `...`, which is refused too.
stopifnot("nchar is not bound" = startsWith(replies[[6L]], "error\t"),
  "an unbound handle is a worker error" = startsWith(replies[[7L]], "error\t"),
  "list is not bound" = startsWith(replies[[8L]], "error\t"),
  "stop is not bound" = startsWith(replies[[10L]], "error\t"))
cat("every failure reaches the binary as an error reply\n")

# base::writeLines writes to standard output when called with its text
# alone, as a project function must not: the worker diverts what it writes
# to standard error, so the protocol stays whole.
errors <- tempfile("yamaa-worker-err-")
replies <- worker(c(paste("bind", enc("base::writeLines"), "str", "text", "con", "sep", "useBytes", sep = "\t"),
  "invoke\t1\ttext=s:shouting", "info"), errors = errors)
expect_identical(replies, c("ok\t1", "ok\t-", paste0("ok\t", enc(yamaa:::.core_version()))),
  "a function that writes to standard output leaves the protocol whole")
expect_identical(readLines(errors), "shouting", "what the function writes reaches standard error")
unlink(errors)
cat("a project function's standard output reaches standard error\n")
