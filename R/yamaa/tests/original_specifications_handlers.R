# Installed original specifications, handler family: missing-value handlers
# and inherited clears, typed intermediate conversions and temporal
# arithmetic, each with its reached handler count, shared with the Rust
# adapter controls and Python.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_handlers.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# How each family states the finding a check or a run reaches, or none:
# inheritance nulls name the first issue's condition, typed intermediates a
# condition among the issues, and temporal arithmetic one issue's phase,
# condition, rule and spec path. `stage` is "check" or "run".
reaches <- list(
  inheritance_nulls = function(issues, truth, stage) {
    condition <- truth[[paste0(stage, "_condition")]]
    if (is.null(condition)) nrow(issues) == 0L else nrow(issues) > 0L && identical(issues$condition[[1]], condition)
  },
  typed_intermediates = function(issues, truth, stage) {
    condition <- truth[[paste0(stage, "_condition")]]
    if (is.null(condition)) nrow(issues) == 0L else condition %in% issues$condition
  },
  temporal_arithmetic = function(issues, truth, stage) {
    expected <- truth[[paste0(stage, "_issue")]]
    if (is.null(expected)) return(nrow(issues) == 0L)
    any(vapply(seq_len(nrow(issues)), function(i)
      issues$phase[i] == expected$phase && issues$condition[i] == expected$condition &&
        issues$rule[i] == expected$rule && identical(issues$spec_paths[[i]], expected$spec_path), logical(1)))
  })
local({
  truth <- list(check_condition="unknown_field")
  for (case in list(
    list(label="a required first issue is absent", conditions=character(), expected=FALSE),
    list(label="the required first issue is present", conditions="unknown_field", expected=TRUE))) {
    issues <- data.frame(condition=case$conditions)
    expect_identical(reaches$inheritance_nulls(issues,truth,"check"),case$expected,case$label)
  }
})
# Each family's handler, whose count a case names as `<handler>_count`,
# whether a case builds, and the line that reports the family passed. A 1.x
# file counts an intermediate's absence under `absent`, a 0.5 file under
# `no_match` (D202), so an inheritance case names the one it reaches.
families <- list(
  inheritance_nulls = list(handler = c("absent", "no_match"),
    builds = function(truth) is.null(truth$run_condition),
    passed = "introducing missing handlers and inherited clears agree across public host boundaries"),
  typed_intermediates = list(handler = "unconvertible",
    builds = function(truth) is.null(truth$run_condition),
    passed = "typed intermediate results convert before numeric matching and retain conversion handlers without Python"),
  temporal_arithmetic = list(handler = "invalid",
    builds = function(truth) !is.null(truth$csv),
    passed = "shared temporal arithmetic conformance preserves civil values and duration handlers without Python"))

for (family in names(families)) local({
  setting <- families[[family]]
  for (index in seq_along(host_truth[[family]])) {
    truth <- host_truth[[family]][[index]]
    label <- sprintf("%s case %d", family, index)
    directory <- tempfile(paste0("yamaa-", family, "-")); dir.create(directory)
    entry <- file.path(directory, "spec.yaml")
    writeBin(charToRaw(truth$spec), entry)
    write_files(directory, truth$parents)
    # Static checks must not access the CSV files, which do not exist yet.
    checked <- check_spec(entry)$issues
    if (!reaches[[family]](checked, truth, "check"))
      fail_expectation(paste(label, "check reaches its authored finding"), checked[c("phase", "condition", "rule")],
        truth[c("check_condition", "check_issue")])
    write_files(directory, truth$inputs)
    result <- run_spec(entry)
    if (!reaches[[family]](result$issues, truth, "run"))
      fail_expectation(paste(label, "run reaches its authored finding"), result$issues[c("phase", "condition", "rule")],
        truth[c("run_condition", "run_issue")])
    if (setting$builds(truth)) {
      stopifnot_case(label, "the run saves" = saved(result))
      expect_identical(rawfile(file.path(directory, "result.csv")), charToRaw(truth$csv), paste(label, "saves the authored CSV"))
    } else {
      stopifnot_case(label, "a failed run has no output" = is.null(result$output), "its save is refused" = save_refused(result),
        "nothing is written" = !file.exists(file.path(directory, "result.csv")))
    }
    observations <- domain_observations(result)
    if (!is.null(observations)) {
      handler <- Filter(function(name) !is.null(truth[[paste0(name, "_count")]]), setting$handler)[[1]]
      expected <- truth[[paste0(handler, "_count")]]
      reached <- reached_count(observations, handler)
      if (!isTRUE(reached == expected))
        fail_expectation(paste(label, "reaches its", handler, "handler the authored number of times"),
          reached, expected)
    }
    unlink(directory, recursive = TRUE)
  }
  cat(setting$passed, "\n", sep = "")
})

restore_path()
