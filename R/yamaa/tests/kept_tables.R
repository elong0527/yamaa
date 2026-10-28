# Installed R memo of a result's tables (#4071): a table binding converts its
# table from Rust on its first read and answers later reads with that data
# frame, until a method of the result runs. The seam no shared reader
# reaches is whether a read reaches the native call, so each case counts the
# calls of the native functions it reads through, each replaced in the
# namespace by one that counts and calls it. A table converts once per
# result; status reads every time; a refused save starts the memo again for
# the result and the members made from it; a streamed run's output is a fresh
# stream on each read and its status advances with the stream; and a run
# restored by readRDS refuses each table as expired rather than answering
# from what the original kept.
library(yamaa)
# The truth is JSON, which jsonlite reads, and the streamed output is read
# with arrow, both suggested packages: where one is not installed, the file is
# skipped.
if (!requireNamespace("jsonlite", quietly = TRUE) || !requireNamespace("arrow", quietly = TRUE)) {
  message("Skipped kept_tables.R: it needs the suggested packages jsonlite, for its truth, and arrow, which reads the stream")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
lifetime <- read_host_fixture(host_fixture_root, "interface")$lifetime
streaming <- read_host_fixture(host_fixture_root, "streaming")

# A new directory holding the lifetime study, or with `records` the large
# streaming study, whose IDs count from 1 and whose names are the prefix and
# the ID.
study <- function(records = NULL) {
  root <- tempfile("yamaa-kept-")
  dir.create(root)
  if (is.null(records)) {
    writeBin(charToRaw(lifetime$specification_yaml), file.path(root, "spec.yaml"))
    writeBin(charToRaw(lifetime$input_csv), file.path(root, "input.csv"))
    writeBin(charToRaw(lifetime$project_yaml), file.path(root, "project.yaml"))
  } else {
    writeBin(charToRaw(streaming$large$specification_yaml), file.path(root, "spec.yaml"))
    ids <- seq_len(records)
    writeLines(c("ID,NAME", paste0(ids, ",", streaming$large$name_prefix, ids)),
      file.path(root, "input.csv"), sep = "\n", useBytes = TRUE)
  }
  root
}
# Run `code` with each native function named in `natives` counting its
# calls, and return the counts by name.
counted <- function(natives, code) {
  calls <- setNames(integer(length(natives)), natives)
  namespace <- asNamespace("yamaa")
  wrapped <- function(index) {
    if (index > length(natives)) return(force(code))
    name <- natives[[index]]
    native <- get(name, namespace)
    with_binding(name, function(...) {
      calls[[name]] <<- calls[[name]] + 1L
      native(...)
    }, wrapped(index + 1L))
  }
  wrapped(1L)
  calls
}
# The value of the binding `name` of `object`, as `object$name` reads it.
read <- function(object, name) get(name, envir = object, inherits = FALSE)
# The message of the yamaa_error `code` signals, or NA when it signals none.
refusal <- function(code) tryCatch({ code; NA_character_ }, yamaa_error = conditionMessage)

check_once <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  spec <- yamaa_spec(file.path(root, "spec.yaml"))
  project <- yamaa_project(file.path(root, "project.yaml"))
  # Each result's tables, by name, with the native function each reads, and
  # the native function of its status.
  cases <- list(
    list(label = "a check", make = function() spec$check(), status = ".native_check_status",
      tables = c(issues = ".native_check_issues", timings = ".native_check_timings")),
    list(label = "a run", make = function() spec$run(), status = ".native_run_status",
      tables = c(output = ".native_run_output", issues = ".native_run_issues",
        report = ".native_run_report", timings = ".native_run_timings")),
    list(label = "a project run", make = function() project$run(), status = ".native_project_run_status",
      tables = c(issues = ".native_project_run_issues", report = ".native_project_run_report",
        timings = ".native_project_run_timings")),
    list(label = "a project run's member", make = function() project$run()[["TEST"]],
      status = ".native_project_run_member_status",
      tables = c(output = ".native_project_run_output", issues = ".native_project_run_member_issues",
        report = ".native_project_run_member_report", timings = ".native_project_run_member_timings")),
    list(label = "a submission", make = function() project$submission(), status = ".native_submission_status",
      tables = c(issues = ".native_submission_issues", timings = ".native_submission_timings")),
    list(label = "a selftest", make = function() yamaa_selftest(), status = ".native_selftest_status",
      tables = c(cases = ".native_selftest_cases")))
  for (case in cases) {
    result <- case$make()
    reads <- c(names(case$tables), "status")
    calls <- counted(c(unname(case$tables), case$status), {
      first <- lapply(reads, read, object = result)
      second <- lapply(reads, read, object = result)
    })
    expect_identical(
      list(tables = vapply(first[seq_along(case$tables)], is.data.frame, logical(1L)),
        again = identical(second, first), calls = calls),
      list(tables = rep(TRUE, length(case$tables)), again = TRUE,
        calls = setNames(c(rep(1L, length(case$tables)), 2L), c(unname(case$tables), case$status))),
      paste(case$label, "converts each table once and reads its status each time"))
  }
  cat("each table of a result converts once, and its status is read each time\n")
}
check_once()

check_refused_save <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run()
  project_run <- yamaa_project(file.path(root, "project.yaml"))$run()
  # A directory where the output goes refuses the save at the output's path
  # (FILE-7), as the diagnostics registry records for a publication target.
  # A project run's own findings also name the listed specification as their
  # entry and source (rules/profiles/project.md), which a member's do not.
  output <- list(path = "output.csv")
  listed <- list(entry = "spec.yaml", path = "output.csv", source = "spec.yaml")
  # Each result, the objects that read its issues (a project run and the
  # member made from it share its memo), the context each names for the
  # refused save, and the native function each reads.
  cases <- list(
    list(label = "a run", result = run, readers = list(run), contexts = list(output),
      natives = ".native_run_issues"),
    list(label = "a project run and its member", result = project_run,
      readers = list(project_run, project_run[["TEST"]]), contexts = list(listed, output),
      natives = c(".native_project_run_issues", ".native_project_run_member_issues")))
  # The issues each reader answers with, each context decoded, and a
  # specification it names as the file name, which is all a test can author
  # of the path the engine spells.
  issues <- function(readers) lapply(readers, function(reader) {
    frame <- reader$issues
    list(phase = frame$phase, condition = frame$condition, rule = frame$rule,
      spec_paths = frame$spec_paths, context = lapply(frame$context, function(text) {
        context <- jsonlite::fromJSON(text, simplifyVector = FALSE)
        for (key in intersect(c("entry", "source"), names(context))) context[[key]] <- basename(context[[key]])
        context
      }))
  })
  kept <- lapply(cases, function(case) issues(case$readers))
  dir.create(file.path(root, "output.csv"))
  for (index in seq_along(cases)) {
    case <- cases[[index]]
    calls <- counted(case$natives, {
      refused <- inherits(tryCatch(case$result$save(), yamaa_error = identity), "yamaa_error")
      after <- issues(case$readers)
      again <- issues(case$readers)
    })
    none <- list(phase = character(), condition = character(), rule = character(),
      spec_paths = list(), context = list())
    added <- lapply(case$contexts, function(context)
      list(phase = "output", condition = "resource_path_not_regular_file", rule = "FILE-7",
        spec_paths = list("output.path"), context = list(context)))
    expect_identical(
      list(before = kept[[index]], refused = refused, after = after, again = again, calls = calls),
      list(before = rep(list(none), length(case$readers)), refused = TRUE, after = added, again = added,
        calls = setNames(rep(1L, length(case$natives)), case$natives)),
      paste("a refused save of", case$label, "starts its kept issues again"))
  }
  cat("a refused save starts the kept issues again, of the result and its members\n")
}
check_refused_save()

check_streamed <- function() {
  root <- study(streaming$large$records)
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run(stream = TRUE)
  # A result asked for while the stream is open refuses, in the engine's
  # words for stream_open after the subject, as the truth's stream_taken
  # refusal reads.
  open <- "cannot read TEST: the run's output stream has not been read to its end [stream_open]"
  calls <- counted(c(".native_run_output", ".native_run_status", ".native_run_issues"), {
    reader <- arrow::RecordBatchReader$import_from_c(run$output)
    first <- reader$read_next_batch()$num_rows
    reading <- list(status = refusal(run$status), issues = refusal(run$issues))
    while (!is.null(reader$read_next_batch())) NULL
    ended <- list(status = run$status, issues = nrow(run$issues), again = nrow(run$issues))
    # Another read of the output is a new stream of the run, which refuses
    # the stream its first reader took.
    again <- tryCatch({
      arrow::RecordBatchReader$import_from_c(run$output)$read_next_batch()
      NA_character_
    }, error = conditionMessage)
  })
  expect_identical(
    list(first = first, reading = reading, ended = ended,
      again = grepl(streaming$stream_taken, again, fixed = TRUE), calls = calls),
    list(first = as.integer(streaming$large$batches[[1L]]), reading = list(status = open, issues = open),
      ended = list(status = "succeeded", issues = 0L, again = 0L), again = TRUE,
      calls = c(.native_run_output = 2L, .native_run_status = 2L, .native_run_issues = 2L)),
    "a streamed run's output is a new stream on each read, and its status advances with the stream")
  cat("a streamed run's output is a new stream on each read, and its status advances with it\n")
}
check_streamed()

check_restored <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run()
  tables <- c("output", "issues", "report", "timings")
  kept <- lapply(setNames(nm = tables), read, object = run)
  saved <- file.path(root, "run.rds")
  saveRDS(run, saved)
  restored <- readRDS(saved)
  seen <- lapply(setNames(nm = tables), function(table) refusal(read(restored, table)))
  expect_identical(
    list(restored = seen, original = identical(lapply(setNames(nm = tables), read, object = run), kept)),
    list(restored = as.list(setNames(rep("expired native handle", length(tables)), tables)), original = TRUE),
    "a run restored by readRDS refuses each kept table as expired")
  cat("a run restored by readRDS refuses each kept table as expired\n")
}
check_restored()
