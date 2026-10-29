# Installed streamed runs (#3697): with stream = TRUE a specification's run
# returns its output as an Arrow C stream, an external pointer of class
# nanoarrow_array_stream, which produces the output as a reader pulls it, one
# batch per pull; the run's other fields give its result once that stream has
# ended, and run it whole when nobody has started to read it. The truth is
# the shared streaming family: the rows and saved CSV of a study of every
# column type, the batches of a 70,000-record study under the engine's
# 65,536-record batches, and the texts of a cancelled streamed run. A
# streamed output also equals the whole-table output of the same
# specification. The end states are #3697's mid-stream row: a reader released
# after its first batch and a project function's interrupt cancel the run,
# which then has no result and publishes nothing.
library(yamaa)
# The truth is JSON, which jsonlite reads, and the stream is read with arrow,
# both suggested packages: where one is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE) || !requireNamespace("arrow", quietly = TRUE)) {
  message("Skipped streaming.R: it needs the suggested packages jsonlite, for its truth, and arrow, which reads the stream")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
projects <- normalizePath("project-base", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "streaming")
large <- truth$large

# A new directory holding the study of every column type, or with `records`
# the large study, whose IDs count from 1 and whose names are the prefix and
# the ID.
study <- function(records = NULL) {
  root <- tempfile("yamaa-streaming-")
  dir.create(root)
  if (is.null(records)) {
    writeBin(charToRaw(truth$specification_yaml), file.path(root, "spec.yaml"))
    writeBin(charToRaw(truth$input_csv), file.path(root, "input.csv"))
  } else {
    writeBin(charToRaw(large$specification_yaml), file.path(root, "spec.yaml"))
    ids <- seq_len(records)
    writeLines(c("ID,NAME", paste0(ids, ",", large$name_prefix, ids)), file.path(root, "input.csv"),
      sep = "\n", useBytes = TRUE)
  }
  root
}
# A table's columns as plain values: numbers as doubles, dates and datetimes
# as their canonical text (VALUE-17), the datetimes read in UTC.
plain <- function(frame) {
  list(
    ID = as.numeric(frame$ID),
    NAME = as.character(frame$NAME),
    V = as.numeric(frame$V),
    D = format(frame$D, "%Y-%m-%d"),
    T = format(frame$T, "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  )
}
# The truth's rows as plain values.
expected <- function() {
  cell <- function(row, column) if (is.null(row[[column]])) NA_character_ else row[[column]]
  list(
    ID = vapply(truth$rows, function(row) as.numeric(row$ID), numeric(1L)),
    NAME = vapply(truth$rows, cell, character(1L), column = "NAME"),
    V = vapply(truth$rows, function(row) if (is.null(row$V)) NA_real_ else as.numeric(row$V), numeric(1L)),
    D = vapply(truth$rows, cell, character(1L), column = "D"),
    T = vapply(truth$rows, cell, character(1L), column = "T")
  )
}
# Read a run's output stream to its end with arrow.
read_stream <- function(stream) as.data.frame(arrow::RecordBatchReader$import_from_c(stream)$read_table())
# The message of the yamaa_error `code` signals.
refusal <- function(code) {
  seen <- tryCatch({ code; NULL }, yamaa_error = identity)
  if (is.null(seen)) return(NA_character_)
  expect_identical(
    list(class = class(seen), frame = is.data.frame(seen$issues), rows = nrow(seen$issues), paths = seen$paths),
    list(class = fixture_strings(truth$refusal$r), frame = TRUE, rows = 0L, paths = character()),
    "streaming state refusals carry an empty issues frame")
  conditionMessage(seen)
}

check_rows <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  spec <- yamaa_spec(file.path(root, "spec.yaml"))
  whole <- spec$run()$output
  run <- spec$run(stream = TRUE)
  stream <- run$output
  streamed <- read_stream(stream)
  saved <- basename(run$save())
  expect_identical(
    list(class = class(stream), streamed = plain(streamed), whole = plain(whole), status = run$status,
      saved = saved, bytes = rawfile(file.path(root, "output.csv"))),
    list(class = fixture_strings(truth$stream_class), streamed = expected(), whole = expected(),
      status = "succeeded", saved = "output.csv", bytes = charToRaw(truth$output_csv)),
    "a streamed output holds the expected rows and saves the expected file")
  cat("a streamed output holds the expected rows and saves the expected file\n")
}
check_rows()

check_batches <- function() {
  root <- study(large$records)
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run(stream = TRUE)
  reader <- arrow::RecordBatchReader$import_from_c(run$output)
  batches <- c()
  while (!is.null(batch <- reader$read_next_batch())) batches <- c(batches, batch$num_rows)
  expect_identical(list(batches = batches, status = run$status),
    list(batches = as.integer(fixture_strings(large$batches)), status = "succeeded"),
    "a large output streams in the engine's batches")
  cat("a large output streams in the engine's batches\n")
}
check_batches()

check_whole <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run(stream = TRUE)
  status <- run$status
  expect_identical(list(status = status, first = plain(read_stream(run$output)), again = plain(read_stream(run$output))),
    list(status = "succeeded", first = expected(), again = expected()),
    "an unread run runs whole when its result is asked for, and its output is then read as often as asked")
  cat("an unread run runs whole when its result is asked for\n")
}
check_whole()

check_released <- function() {
  root <- study(large$records)
  on.exit(unlink(root, recursive = TRUE))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run(stream = TRUE)
  reader <- arrow::RecordBatchReader$import_from_c(run$output)
  first <- reader$read_next_batch()$num_rows
  rm(reader)
  invisible(gc())
  expect_identical(
    list(first = first, status = refusal(run$status), save = refusal(run$save()),
      published = file.exists(file.path(root, "output.csv"))),
    list(first = as.integer(large$batches[[1L]]), status = truth$cancelled, save = truth$cancelled,
      published = FALSE),
    "a released reader cancels the run, which publishes nothing")
  cat("a released reader cancels the run and publishes nothing\n")
}
check_released()

check_interrupt <- function() {
  root <- tempfile("yamaa-streaming-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  case <- function_study(projects, root)
  spec <- yamaa_project(file.path(case, "project.yaml"))$spec("TEST")
  capabilities <- get(".yamaa_locked_host_capabilities", asNamespace("yamaa"))()
  interrupted <- structure(list(message = "interrupted", call = NULL), class = c("interrupt", "condition"))
  # The project function signals an interrupt each time it is called.
  resolve <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    list(0L, function(arguments) list(3L, interrupted))
  }
  # The run keeps the capabilities it was started with, and its build, at
  # the stream's first pull, calls them.
  run <- with_capabilities(capabilities$verify, resolve, spec$run(stream = TRUE))
  reader <- arrow::RecordBatchReader$import_from_c(run$output)
  pulled <- tryCatch({ reader$read_next_batch(); NA_character_ }, error = conditionMessage)
  expect_identical(
    list(pull = grepl(truth$cancelled, pulled, fixed = TRUE), status = refusal(run$status),
      published = file.exists(file.path(case, "test.csv"))),
    list(pull = TRUE, status = truth$cancelled, published = FALSE),
    "a project function's interrupt cancels the stream, whose run publishes nothing")
  cat("a project function's interrupt cancels the stream\n")
}
check_interrupt()
