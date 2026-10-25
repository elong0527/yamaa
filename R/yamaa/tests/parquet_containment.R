# Installed Parquet refusal: two files of this family are invalid. In one, an
# INT96 column statistic holds 13 bytes where the type's statistics hold 12.
# In the other, two row groups name one page whose header holds fields that
# parquet 55 could not skip, which its path refused and the native reader
# still refuses. The engine refuses each as source_parquet_invalid
# (PARQUET-5), prints nothing, and later reads in the same process still
# work. Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped parquet_containment.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
truth <- read_host_fixture(host_fixture_root, "parquet-containment")

work <- tempfile("yamaa-parquet-containment-"); dir.create(work)
spec_path <- function(name) {
  directory <- file.path(work, name); dir.create(directory)
  writeBin(charToRaw(enc2utf8(truth[["specification_yaml"]])), file.path(directory, "spec.yaml"))
  writeBin(fixture_bytes(truth[[name]]), file.path(directory, "input.parquet"))
  normalizePath(file.path(directory, "spec.yaml"), winslash = "/", mustWork = TRUE)
}
invalid <- c(long_int96_statistic = spec_path("long_int96_statistic_parquet"),
  shared_presented_page = spec_path("shared_presented_page_parquet"))
readable <- spec_path("readable_parquet")
saved <- file.path(dirname(readable), "output.csv")
readable_csv <- charToRaw(enc2utf8(truth[["readable_csv"]]))
saved_csv <- function() readBin(saved, "raw", file.size(saved))

# Field for field, as public_contract.R compares the shared parity truth. A
# truth row names its file relative to the specification's directory; a
# location names it by its absolute path (P-ISSUE-1).
for (case in names(invalid)) for (attempt in 1:2) {
  result <- yamaa_spec(invalid[[case]])$run()
  failure <- tryCatch({result$save(); NULL}, yamaa_error = identity)
  label <- sprintf("run %d of the %s file", attempt, case)
  expect_identical(issue_columns_of(result$issues), issue_truth(truth[["issues"]], dirname(invalid[[case]])),
    paste(label, "has the shared issue rows"))
  stopifnot_case(label, "the invalid file leaves no output" = is.null(result$output),
    "its save signals yamaa_error" = inherits(failure, "yamaa_error"))
  result <- yamaa_spec(readable)$run()
  stopifnot_case(sprintf("run %d of the readable file", attempt), "the run has no issues" = nrow(result$issues) == 0L,
    "the run saves" = returns_paths(result$save()), "the saved CSV is the authored one" = identical(saved_csv(), readable_csv))
}
cat("each invalid file is source_parquet_invalid, and later reads in the process still work\n")

# A fresh R process runs both specifications; its stderr is the one a panic
# hook or a warning would write to. It saves the run that has output and prints each run's
# conditions.
unlink(saved)
child <- file.path(work, "child.R")
writeLines(c(
  paste0(".libPaths(", paste(deparse(.libPaths()), collapse = ""), ")"),
  "suppressPackageStartupMessages(library(yamaa))",
  "for (path in commandArgs(trailingOnly = TRUE)) {",
  "  result <- yamaa_spec(path)$run()",
  "  if (!is.null(result$output)) result$save()",
  "  writeLines(paste(result$issues$condition, collapse = \" \"))",
  "}"), child)
quote <- function(path) shQuote(path, type = if (.Platform$OS.type == "windows") "cmd" else "sh")
out <- file.path(work, "child-stdout.txt"); err <- file.path(work, "child-stderr.txt")
rscript <- file.path(R.home("bin"), if (.Platform$OS.type == "windows") "Rscript.exe" else "Rscript")
# R CMD check names its test startup file in R_TESTS, relative to its own
# directory; the child needs none.
tests <- Sys.getenv("R_TESTS", unset = NA)
Sys.setenv(R_TESTS = "")
status <- system2(rscript, c("--vanilla", quote(child), unname(vapply(invalid, quote, "")), quote(readable)),
  stdout = out, stderr = err)
if (is.na(tests)) Sys.unsetenv("R_TESTS") else Sys.setenv(R_TESTS = tests)
printed <- readLines(err)
if (length(printed)) cat(printed, sep = "\n")
conditions <- vapply(truth[["issues"]], function(issue) issue[["condition"]], "")
stopifnot("the child process succeeds" = identical(status, 0L),
  "refusing the invalid files prints nothing to stderr" = file.size(err) == 0)
expect_identical(readLines(out), c(rep(paste(conditions, collapse = " "), length(invalid)), ""),
  "the child prints each run's conditions")
expect_identical(saved_csv(), readable_csv, "the child saves the readable run")
cat("refusing the invalid files prints nothing to stderr\n")
unlink(work, recursive = TRUE)
