# Installed runs beyond the row limit under the default partitions (#3794).
# The R host runs every build over key-bounded partitions by default, so a CSV
# input of more records than a table may hold runs when its steps are safe
# under those partitions. The truth is the shared `partitioned-default`
# family: a study of 10,000,001 records, whose window numbers each subject's
# records, and the line count and first and last lines of its saved CSV,
# written by hand from its value rule. The Python
# `installed_partitioned_default` suite reads it too.
library(yamaa)
# The input and the run take minutes and gigabytes, so the suite runs only in
# native qualification, whose supplemental inventory sets
# YAMAA_SUPPLEMENTAL_EVIDENCE; other runs keep the smaller control,
# longitudinal_capacity.R, which reads the default partitions from its run's
# observations.
if (!nzchar(Sys.getenv("YAMAA_SUPPLEMENTAL_EVIDENCE"))) {
  message("Skipped partitioned_default.R: 10,000,001 records run only in native qualification's supplemental inventory")
  quit(save = "no")
}
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped partitioned_default.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "partitioned-default")

run_partitioned_default <- function() {
  root <- tempfile("partitioned-default-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  entry <- file.path(root, "spec.yaml")
  writeBin(charToRaw(truth$specification), entry)
  # Record `i`, counted from 0, is study S1, its subject `i` divided by the
  # records each subject has, and SEQ the remainder plus 1; a million records
  # are written at a time.
  input <- file(file.path(root, "input.csv"), "wb")
  writeLines(truth$header, input, sep = "\n", useBytes = TRUE)
  each <- truth$subject_records
  for (start in seq(0, truth$records - 1, by = 1e6)) {
    record <- seq(start, min(truth$records, start + 1e6) - 1)
    writeLines(sprintf("S1,%08.0f,%.0f", record %/% each, record %% each + 1),
      input, sep = "\n", useBytes = TRUE)
  }
  close(input)
  built <- yamaa_spec(entry)$run()
  stopifnot_case("the window beyond the row limit", "the run has no issues" = nrow(built$issues) == 0L)
  stopifnot_case("the window beyond the row limit", "the run saves" = returns_paths(built$save()))
  rm(built)
  # The saved file, read a million lines at a time: its line count, its first
  # three lines and its last two.
  saved <- file(file.path(root, "out.csv"), "rb")
  on.exit(close(saved), add = TRUE, after = FALSE)
  count <- 0
  first <- character()
  last <- character()
  repeat {
    lines <- readLines(saved, n = 1e6)
    if (length(lines) == 0L) break
    count <- count + length(lines)
    if (length(first) < 3L) first <- c(first, lines)[seq_len(min(3L, length(first) + length(lines)))]
    last <- utils::tail(c(last, lines), 2L)
  }
  expect_identical(list(count = count, first = first, last = last),
    list(count = as.double(truth$saved_lines), first = unlist(truth$first_lines),
         last = unlist(truth$last_lines)),
    "the saved file of the window beyond the row limit")
}

run_partitioned_default()
