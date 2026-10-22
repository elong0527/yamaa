# Installed compact flow verification parity and refusal controls (#2196), and
# the raw NUL refusal (#3125). The cases and their expected results are the
# `compact-yaml` family of the shared host truth, which the Python
# `installed_compact_yaml` suite reads too.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped compact_yaml.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "compact-yaml")

check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

run_compact_yaml <- function() {
  root <- tempfile("yamaa-compact-yaml-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  writeBin(fixture_bytes(truth$input_csv), file.path(root, "input.csv"))
  spec <- file.path(root, "spec.yaml")
  write_spec <- function(verification, specification = truth$specification_yaml) {
    writeBin(fixture_bytes(paste0(specification, verification, "\n")), spec)
  }
  equivalent <- truth$equivalent
  inputs <- list("a block input" = truth$specification_yaml, "a flow input" = truth$flow_input_yaml)
  for (input_form in names(inputs)) for (case in equivalent$cases) {
    write_spec(case$verification, inputs[[input_form]])
    label <- paste(case$name, "after", input_form)
    stopifnot_case(label, "the variant checks" = nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot_case(label, "the variant runs" = nrow(built$issues) == 0L)
    expect_identical(as.list(built$output), lapply(equivalent$output, fixture_strings),
      paste(label, "keeps every row"))
    stopifnot_case(label, "it saves" = returns_paths(built$save()))
    expect_identical(rawfile(file.path(root, "out.csv")), fixture_bytes(equivalent$output_csv),
      paste(label, "saves the same bytes"))
  }
  for (case in truth$failed_bounds) {
    write_spec(case$verification)
    stopifnot_case(case$name, "the bounds check" = nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot_case(case$name, "a failed count leaves no output" = is.null(built$output))
    expect_identical(built$issues$condition, fixture_strings(case$conditions),
      paste("the bounds of", case$name, "are executed"))
  }
  for (case in truth$malformed) {
    write_spec(case$verification)
    for (operation in c("check", "run")) {
      expect_identical(yamaa_spec(spec)[[operation]]()$issues$condition,
        fixture_strings(case$conditions), paste(operation, "of", case$name))
    }
  }
  # A raw NUL used to end the specification silently, dropping the verification
  # after it. R strings cannot hold NUL, so the bytes are written directly.
  for (case in truth$raw_nul) {
    writeBin(c(fixture_bytes(case$before_nul), as.raw(0L), fixture_bytes(case$after_nul)), spec)
    for (operation in c("check", "run")) {
      issues <- yamaa_spec(spec)[[operation]]()$issues
      expect_identical(as.list(issues[names(case$issue)]), case$issue,
        paste(operation, "of", case$name))
    }
  }
}

run_compact_yaml()
