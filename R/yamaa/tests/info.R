# Installed yamaa_info(): what the engine is and where it runs (D41). The
# facade's own tests cover each field; Python reads the same truth.
# yamaa_info() and a selftest's info are one construction in the binding, so
# they are identical; selftest.R compares that record with the facts it
# derives from this R.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped info.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "info")

stopifnot("yamaa_info() takes no arguments" = length(formals(yamaa_info)) == 0L,
  "yamaa_info() is exported" = "yamaa_info" %in% getNamespaceExports("yamaa"))
cat("yamaa_info() takes no arguments and is exported\n")

info <- yamaa_info()
stopifnot("the information is a list" = is.list(info))
expect_identical(names(info), fixture_strings(truth[["fields"]]), "the information's fields in order")
expect_identical(info[c("version", "language_versions", "features", "host", "host_version")],
  list(version = yamaa:::.core_version(), language_versions = fixture_strings(truth[["language_versions"]]),
    features = fixture_strings(truth[["features"]]), host = "r", host_version = as.character(getRversion())),
  "the engine information of this R")
stopifnot("the host is one of the vocabulary" = info$host %in% fixture_strings(truth[["hosts"]]),
  "the platform is one string" = is.character(info$platform) && length(info$platform) == 1L)
cat("yamaa_info() gives the fields in order\n")

expect_identical(yamaa_info(), yamaa_selftest()$info, "yamaa_info() is a selftest's info")
cat("yamaa_info() equals the information of a selftest\n")
