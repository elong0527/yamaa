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
truth <- read_host_fixture(host_fixture_root, "selftest")
info_truth <- read_host_fixture(host_fixture_root, "info")
statuses <- fixture_strings(truth[["statuses"]])
cases <- fixture_strings(truth[["cases"]])
passed <- truth[["status"]]

stopifnot(length(formals(yamaa_selftest)) == 0L)
result <- yamaa_selftest()
stopifnot(inherits(result, "yamaa_selftest"),
  identical(sort(ls(result), method = "radix"), c("cases", "info", "status")),
  # names() lists the public members in declaration order, and length()
  # counts them (D44).
  identical(names(result), fixture_strings(truth[["r_names"]])), identical(length(result), 3L))
cat("yamaa_selftest() takes no arguments and returns a selftest\n")

stopifnot(is.character(result$status), length(result$status) == 1L,
  result$status %in% statuses, identical(result$status, passed))
cat("every bundled case passes\n")

frame <- result$cases
stopifnot(is.data.frame(frame), identical(names(frame), fixture_strings(truth[["columns"]])),
  all(vapply(frame, is.character, logical(1))), identical(frame$case, cases),
  all(frame$status %in% statuses), identical(frame$status, rep(passed, length(cases))),
  identical(frame$message, rep(NA_character_, length(cases))))
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
stopifnot(identical(names(result$info), fixture_strings(info_truth[["fields"]])),
  identical(result$info, expected_info))
cat("info gives the engine information of this host\n")

information <- truth[["information_line"]]
for (field in c("version", "host", "host_version", "platform")) {
  information <- sub(paste0("{", field, "}"), expected_info[[field]], information, fixed = TRUE)
}
information <- sub("{language_versions}", paste(expected_info$language_versions, collapse = ", "),
  information, fixed = TRUE)
lines <- c(fixture_strings(truth[["lines"]]), information)
stopifnot(identical(format(result), lines))
# print() writes each line ended by a line feed, and returns the result
# invisibly.
sunk <- rawConnection(raw(), "w")
sink(sunk)
shown <- withVisible(print(result))
sink()
printed <- rawConnectionValue(sunk)
close(sunk)
stopifnot(identical(printed, charToRaw(paste0(paste(lines, collapse = "\n"), "\n"))),
  identical(shown$value, result), !shown$visible)
cat("printing gives the facade's report byte for byte\n")

for (name in c("status", "cases", "info")) {
  stopifnot(inherits(tryCatch({ assign(name, NULL, envir = result); NULL }, error = identity),
    "error"))
}
cat("the result is read-only\n")
