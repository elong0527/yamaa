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
truth <- read_host_fixture(host_fixture_root, "info")

stopifnot(length(formals(yamaa_info)) == 0L, "yamaa_info" %in% getNamespaceExports("yamaa"))
cat("yamaa_info() takes no arguments and is exported\n")

info <- yamaa_info()
stopifnot(is.list(info), identical(names(info), fixture_strings(truth[["fields"]])),
  identical(info$version, yamaa:::.core_version()),
  identical(info$language_versions, fixture_strings(truth[["language_versions"]])),
  identical(info$features, fixture_strings(truth[["features"]])),
  info$host %in% fixture_strings(truth[["hosts"]]), identical(info$host, "r"),
  identical(info$host_version, as.character(getRversion())),
  is.character(info$platform), length(info$platform) == 1L)
cat("yamaa_info() gives the fields in order\n")

stopifnot(identical(yamaa_info(), yamaa_selftest()$info))
cat("yamaa_info() equals the information of a selftest\n")
