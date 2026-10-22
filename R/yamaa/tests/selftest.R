# Installed yamaa_selftest(): the bundled cases through the installed engine.
# The facade's own tests run every case and each way a case can fail; Python
# reads the same truth. A shipped build compiles every feature, so every case
# runs and passes.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped selftest.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "selftest")
info_truth <- read_host_fixture(host_fixture_root, "info")
statuses <- fixture_strings(truth[["statuses"]])
cases <- fixture_strings(truth[["cases"]])
passed <- truth[["status"]]

stopifnot("yamaa_selftest() takes no arguments" = length(formals(yamaa_selftest)) == 0L)
result <- yamaa_selftest()
stopifnot("the result is a selftest" = inherits(result, "yamaa_selftest"))
expect_identical(sort(ls(result), method = "radix"), c("cases", "info", "status"), "a selftest's members")
# names() lists the public members in declaration order, and length()
# counts them (D44).
expect_identical(names(result), fixture_strings(truth[["r_names"]]), "a selftest names its members")
expect_identical(length(result), 3L, "a selftest counts its members")
cat("yamaa_selftest() takes no arguments and returns a selftest\n")

stopifnot("the status is one of the vocabulary" =
  is.character(result$status) && length(result$status) == 1L && result$status %in% statuses)
expect_identical(result$status, passed, "every bundled case passes")
cat("every bundled case passes\n")

frame <- result$cases
stopifnot("the cases are a data frame" = is.data.frame(frame),
  "every column is character" = all(vapply(frame, is.character, logical(1))),
  "every status is one of the vocabulary" = all(frame$status %in% statuses))
expect_identical(names(frame), fixture_strings(truth[["columns"]]), "the cases' columns")
expect_identical(as.list(frame[c("case", "status", "message")]),
  list(case = cases, status = rep(passed, length(cases)), message = rep(NA_character_, length(cases))),
  "the bundled cases pass in list order")
cat("cases has one row per case in list order\n")

# How Rust names the operating system and architecture of the compiled engine,
# from the names R reports for its own platform.
systems <- c(Linux = "linux", Darwin = "macos", Windows = "windows")
architectures <- c(x86_64 = "x86_64", aarch64 = "aarch64", arm64 = "aarch64")
expected_info <- list(
  version = yamaa:::.core_version(),
  language_versions = fixture_strings(info_truth[["language_versions"]]),
  features = fixture_strings(info_truth[["features"]]),
  host = "r",
  host_version = as.character(getRversion()),
  platform = paste0(systems[[Sys.info()[["sysname"]]]], "-", architectures[[R.version$arch]])
)
expect_identical(names(result$info), fixture_strings(info_truth[["fields"]]), "the info's fields")
expect_identical(result$info, expected_info, "the engine information of this host")
cat("info gives the engine information of this host\n")

information <- truth[["information_line"]]
for (field in c("version", "host", "host_version", "platform")) {
  information <- sub(paste0("{", field, "}"), expected_info[[field]], information, fixed = TRUE)
}
information <- sub("{language_versions}", paste(expected_info$language_versions, collapse = ", "),
  information, fixed = TRUE)
lines <- c(fixture_strings(truth[["lines"]]), information)
expect_identical(format(result), lines, "a selftest formats as its report")
# print() writes each line ended by a line feed, and returns the result
# invisibly.
sunk <- rawConnection(raw(), "w")
sink(sunk)
shown <- withVisible(print(result))
sink()
printed <- rawConnectionValue(sunk)
close(sunk)
expect_identical(printed, charToRaw(paste0(paste(lines, collapse = "\n"), "\n")), "a selftest prints its report")
stopifnot("print() returns the result" = identical(shown$value, result),
  "print() returns it invisibly" = !shown$visible)
cat("printing gives the facade's report byte for byte\n")

for (name in c("status", "cases", "info")) {
  stopifnot_case(name, "the member is read-only" =
    inherits(tryCatch({ assign(name, NULL, envir = result); NULL }, error = identity), "error"))
}
cat("the result is read-only\n")
