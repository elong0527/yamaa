# Installed .yamaa_worker(): the worker the standalone yamaa command line
# runs its R project functions through (cli.worker of
# rust/interface/bindings.yaml). The binary's own tests cover its side of
# the protocol, the codec of rust/crates/yamaa-cli/src/worker.rs; this
# drives the worker as the binary does, over its standard input and output,
# and checks what it answers: the installed engine's version, the renv lock
# findings, a bound callable's results row by row and as a batch, and the
# failures it reports in the engine's own terms. Python reads the same seam.
library(yamaa)
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)

enc <- yamaa:::.worker_encode
dec <- yamaa:::.worker_decode

# The replies to `lines`, then `exit`, from a worker run as Rscript runs it.
worker <- function(lines) {
  input <- tempfile("yamaa-worker-in-")
  writeLines(c(lines, "exit"), input)
  replies <- system2(file.path(R.home("bin"), "Rscript"),
    c("-e", shQuote("yamaa:::.yamaa_worker()")), stdin = input, stdout = TRUE, stderr = FALSE)
  stopifnot("the worker answers every request and the exit" = identical(replies[[length(replies)]], "ok"))
  replies[-length(replies)]
}

expect_identical(enc("base::signif"), "base%3A%3Asignif", "text percent-encodes every byte outside the unreserved set")
expect_identical(dec("a%20b%2F%C3%A9"), "a b/\u00e9", "percent-encoded UTF-8 decodes to the text")
expect_identical(enc("a b/\u00e9"), "a%20b%2F%C3%A9", "the encoding round-trips")
cat("the codec spells text as the protocol says\n")

encode <- yamaa:::.worker_encode_value
expect_identical(encode(NULL, "float"), "-", "NULL is missing")
expect_identical(encode(NA_real_, "float"), "-", "NA is missing")
expect_identical(encode(Inf, "float"), "-", "a non-finite double is missing")
expect_identical(encode(2.5, "float"), "f:2.5", "a float spells as a round-trip decimal")
expect_identical(encode(3, "int"), "i:3", "a whole double is an int")
expect_identical(encode(3L, "int"), "i:3", "an integer is an int")
expect_identical(encode("a b", "str"), "s:a%20b", "a string is percent-encoded")
expect_identical(encode(as.Date("2024-02-29"), "date"), "d:2024-02-29", "a Date spells its day")
expect_identical(encode(as.POSIXct("2024-02-29 23:59:58", tz = "UTC"), "datetime"),
  "t:2024-02-29T23:59:58", "a POSIXct spells its second in UTC")
for (case in list(list(2.5, "int"), list("a", "int"), list(c(1, 2), "float"), list(TRUE, "str"),
  list(as.Date("2024-02-29"), "float"), list(1, "date"))) {
  rejected <- tryCatch(encode(case[[1L]], case[[2L]]), condition = identity)
  stopifnot_case(paste("the", case[[2L]], "result", deparse(case[[1L]])),
    "the result is rejected" = inherits(rejected, "yamaa_worker_rejected"))
}
decode <- yamaa:::.worker_decode_value
expect_identical(decode("-"), NA, "a missing argument is NA")
expect_identical(decode("s:a%20b"), "a b", "a str argument decodes")
expect_identical(decode("i:3"), 3, "an int argument is a double")
expect_identical(decode("f:inf"), Inf, "an infinite float decodes")
expect_identical(decode("b:true"), TRUE, "a bool argument is a logical")
expect_identical(decode("d:2024-02-29"), as.Date("2024-02-29"), "a date argument is a Date")
expect_identical(decode("t:2024-02-29T23:59:58"), as.POSIXct("2024-02-29 23:59:58", tz = "UTC"),
  "a datetime argument is a POSIXct in UTC")
cat("the codec converts values as the in-process port does\n")

replies <- worker("info")
expect_identical(replies, paste0("ok\t", enc(yamaa:::.core_version())), "the worker names the installed engine")
cat("the worker names the installed engine\n")

replies <- worker(c(
  paste("bind", enc("base::signif"), "float", "x", "digits", sep = "\t"),
  "invoke\t1\tx=f:2.375\tdigits=i:3",
  "batch\t1\t2\t2",
  "x\tf:2.375\tf:-1.115",
  "digits\ti:3\ti:3",
  "invoke\t1\tx=-\tdigits=i:3",
  paste("bind", enc("base::no_such_function"), "str", "x", sep = "\t"),
  paste("bind", enc("base::paste"), "str", "x", sep = "\t"),
  paste("bind", enc("base::nchar"), "int", "x", sep = "\t"),
  "invoke\t2\tx=s:abc",
  paste("bind", enc("base::list"), "str", sep = "\t"),
  "invoke\t3",
  paste("bind", enc("base::stop"), "str", sep = "\t")))
expect_identical(replies[1:3], c("ok\t1", "ok\tf:2.3799999999999999", "ok\tf:2.3799999999999999\tf:-1.1200000000000001"),
  "a bound function answers each call and each row of a batch")
expect_identical(replies[[4L]], "ok\t-", "a missing argument gives a missing result")
stopifnot("an unknown callable is reported as an error" = startsWith(replies[[5L]], "error\t"))
expect_identical(replies[[6L]], paste0("error\tsimpleError\t", enc("installed callable signature differs from params")),
  "a callable whose signature differs from the parameters is not bound")
cat("the worker binds installed callables and reports binding failures\n")

# nchar's formals differ from one parameter, so it is not bound; list() and
# stop() take only `...`, which is refused too.
stopifnot("nchar is not bound" = startsWith(replies[[7L]], "error\t"),
  "an unbound handle is a worker error" = startsWith(replies[[8L]], "error\t"),
  "list is not bound" = startsWith(replies[[9L]], "error\t"),
  "stop is not bound" = startsWith(replies[[11L]], "error\t"))
cat("every failure reaches the binary as an error reply\n")
