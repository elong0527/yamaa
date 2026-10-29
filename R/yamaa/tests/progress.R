# Installed progress and timings (#3762): `progress =` on a run is called with
# one named list per progress event, in order, and every result's `$timings`
# is a data frame of the phases it ran. The truth is the shared progress
# family: a study, the events a run of it reports, the timings table's
# columns and the phases a run and a check list, and the issue rows of a run
# whose callback signalled an error. Such a run fails and saves nothing, and a
# callback may run the same specification again through the same object.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped progress.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "progress")

# A new directory holding the study.
study <- function() {
  root <- tempfile("yamaa-progress-")
  dir.create(root)
  writeBin(charToRaw(truth$specification_yaml), file.path(root, "spec.yaml"))
  writeBin(charToRaw(truth$input_csv), file.path(root, "input.csv"))
  root
}
# The truth's events, `{path}` standing for the path the test gave, each as
# the named list R receives: a missing field is NULL, a count a double.
expected_events <- function(path) {
  lapply(truth$events, function(event) {
    count <- function(value) if (is.null(value)) NULL else as.double(value)
    list(name = if (identical(event$name, "{path}")) path else event$name,
      phase = event$phase, kind = event$kind, outcome = event$outcome,
      number = count(event$number), rows = count(event$rows))
  })
}

root <- study()
path <- file.path(root, "spec.yaml")
spec <- yamaa_spec(path)

heard <- list()
run <- spec$run(progress = function(event) heard[[length(heard) + 1L]] <<- event)
expect_identical(run$status, "succeeded", "a run with progress")
expect_identical(heard, expected_events(path), "the events a run reports")
cat("a run reports each event in order as a named list\n")

# A new object, so the run compiles its plan rather than reuse the one kept.
fresh <- yamaa_spec(path)
for (case in list(
  list(label = "a run", result = fresh$run(), phases = truth$run_phases),
  list(label = "a check", result = fresh$check(), phases = truth$check_phases))) {
  timings <- case$result$timings
  phases <- as.character(unlist(case$phases))
  expect_identical(names(timings), as.character(unlist(truth$timing_columns)),
    paste0(case$label, ": the timings columns"))
  expect_identical(list(timings$name, timings$phase, timings$outcome),
    list(rep(path, length(phases)), phases, rep("succeeded", length(phases))),
    paste0(case$label, ": the phases it ran"))
  stopifnot_case(case$label, "the times are not negative" =
    all(timings$started_ms >= 0) && all(timings$elapsed_ms >= 0))
}
cat("a run and a check give the timings of the phases they ran\n")

calls <- 0L
original <- structure(list(message = truth$callback_failure$message, call = NULL, identity = new.env()),
  class = c(truth$callback_failure$class, "error", "condition"))
failed <- spec$run(progress = function(event) {
  calls <<- calls + 1L
  stop(original)
})
expect_identical(calls, 1L, "a failing callback is called once, then no more")
expect_identical(list(failed$status, failed$output), list("failed", NULL), "a run whose callback failed")
expect_identical(issue_columns_of(failed$issues), issue_truth(truth$failed_run_issues, root),
  "a run whose callback failed: its issues")
refused <- tryCatch({ failed$save(); NULL }, yamaa_error = function(condition) condition)
stopifnot("the save of a run whose callback failed is refused" = !is.null(refused))
expect_identical(refused$paths, character(), "the refused save wrote no path")
# The native payload retains arbitrary condition fields and environment identity.
expect_identical(refused$parent, original, "the refusal chains the original callback condition")
expect_identical(sort(list.files(root)), c("input.csv", "spec.yaml"), "nothing is written")
cat("a callback that signals an error fails the run, which saves nothing\n")

inner <- NULL
again <- spec$run(progress = function(event) {
  if (identical(event$phase, "derivation") && is.null(inner)) inner <<- spec$run()$status
})
expect_identical(list(again$status, inner), list("succeeded", "succeeded"),
  "a callback that runs the same object again")
cat("a callback may run the same object again\n")

refusal <- tryCatch({ spec$run(progress = "not a function"); NULL }, error = conditionMessage)
expect_identical(refusal, "`progress` must be a function or NULL", "a progress that is not a function")
cat("a progress that is not a function is refused\n")
unlink(root, recursive = TRUE)
