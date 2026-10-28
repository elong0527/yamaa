# Installed Parquet reading: the types pyarrow writes by default read.
# pyarrow 26 wrote this family's file from a table with a timestamp in
# nanoseconds, one in milliseconds and a single-precision float, the types
# pandas and pyarrow write by default. Each reads as its column type
# (PARQUET-7): the timestamps as the whole seconds they count (PARQUET-9) and
# the float as the binary64 value it names, so the saved CSV holds that
# value's text. Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped parquet_writer_defaults.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
truth <- read_host_fixture(host_fixture_root, "parquet-writer-defaults")

work <- tempfile("yamaa-parquet-defaults-"); dir.create(work)
writeBin(charToRaw(enc2utf8(truth[["specification_yaml"]])), file.path(work, "spec.yaml"))
writeBin(fixture_bytes(truth[["writer_defaults_parquet"]]), file.path(work, "input.parquet"))
spec <- normalizePath(file.path(work, "spec.yaml"), winslash = "/", mustWork = TRUE)
saved <- file.path(work, "output.csv")
saved_csv <- charToRaw(enc2utf8(truth[["saved_csv"]]))

result <- yamaa_spec(spec)$run()
stopifnot_case("the file pyarrow wrote", "the run has no issues" = nrow(result$issues) == 0L,
  "the run saves" = returns_paths(result$save()))
expect_identical(readBin(saved, "raw", file.size(saved)), saved_csv,
  "the saved CSV is the authored one")
cat("the timestamp and float types pyarrow writes by default read and save\n")
unlink(work, recursive = TRUE)
