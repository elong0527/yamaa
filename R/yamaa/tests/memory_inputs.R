# Owned input snapshots from the shared memory-inputs truth (#4086).
library(yamaa)
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped memory_inputs.R: jsonlite is not installed")
  quit(save = "no")
}
host <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host, "reader.R"), local = TRUE)
source(file.path(host, "assert.R"), local = TRUE)
source(file.path(host, "context.R"), local = TRUE)
truth <- read_host_fixture(host, "memory-inputs")
write_text <- function(path, value) writeBin(charToRaw(enc2utf8(value)), path)
read_text <- function(path) rawToChar(readBin(path, "raw", n = file.info(path)$size))
frame_of <- function(case) {
  columns <- lapply(seq_along(truth$columns), function(column)
    vapply(case$rows, function(row) as.integer(row[[column]]), integer(1)))
  as.data.frame(setNames(columns, unlist(truth$columns)), check.names = FALSE)
}
typed_frame <- function() {
  columns <- lapply(truth$typed$columns, function(column) {
    if (identical(column$kind, "int")) {
      return(vapply(column$values, function(value) if (is.null(value)) NA_integer_ else as.integer(value), integer(1)))
    }
    if (identical(column$kind, "float")) {
      return(vapply(column$values, function(value) if (is.null(value)) NA_real_ else as.double(value), 0))
    }
    values <- vapply(column$values, function(value) if (is.null(value)) NA_character_ else value, "")
    switch(column$kind, str = values, date = as.Date(values),
      datetime = as.POSIXct(values, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC"))
  })
  as.data.frame(setNames(columns, vapply(truth$typed$columns, `[[`, "", "name")),
    check.names = FALSE, stringsAsFactors = FALSE)
}
assert_issue <- function(frame, expected, label) {
  expect_identical(nrow(frame), 1L, label)
  expect_identical(list(phase = frame$phase[[1L]], condition = frame$condition[[1L]],
      rule = frame$rule[[1L]], spec_path = frame$spec_paths[[1L]], message = frame$message[[1L]],
      source_span = frame$source_span[[1L]], operand_route = frame$operand_route[[1L]]),
    list(phase = expected$phase, condition = expected$condition, rule = expected$rule,
      spec_path = as.character(unlist(expected$spec_path)), message = expected$message,
      source_span = NULL, operand_route = NULL), label)
  expect_context(jsonlite::fromJSON(frame$context[[1L]], simplifyVector = FALSE),
    expected$context, paste(label, "context"))
}
assert_saved <- function(run, root, csv, label) {
  expect_identical(list(status = run$status, issues = nrow(run$issues)),
    list(status = "succeeded", issues = 0L), label)
  run$save()
  expect_identical(read_text(file.path(root, "out.csv")), csv, label)
}
assert_memory <- function(run) {
  row <- run$report[run$report$KIND == "input" & run$report$NAME == truth$alias, , drop = FALSE]
  expect_context(jsonlite::fromJSON(row$DETAILS[[1L]], simplifyVector = FALSE),
    truth$memory_details, "the report identifies the memory snapshot")
}
refused <- function(call) {
  seen <- tryCatch({ force(call); NULL }, yamaa_error = identity)
  stopifnot_case("host input admission", "a native refusal is signalled" = inherits(seen, "yamaa_error"),
    "its issues are a frame" = is.data.frame(seen$issues))
  seen
}
qualify_memory_inputs <- function() {
  root <- tempfile("yamaa-memory-inputs-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  restore_path <- without_python(); on.exit(restore_path(), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  for (name in names(truth$files)) write_text(file.path(root, name), truth$files[[name]])
  spec <- yamaa_spec(file.path(root, truth$entry))
  for (case in truth$cases) {
    frame <- frame_of(case)
    rownames(frame) <- paste0("ignored-", seq_len(nrow(frame)))
    run <- spec$run(inputs = setNames(list(frame), truth$alias))
    assert_saved(run, root, case$csv, case$name)
    assert_memory(run)
  }
  assert_saved(spec$run(), root, truth$file_csv, "a later run returns to files")
  original <- frame_of(truth$cases[[1L]])
  deferred <- spec$run(stream = TRUE, inputs = list(SRC = original))
  original$VALUE[[1L]] <- as.integer(truth$cases[[2L]]$rows[[1L]][[2L]])
  assert_saved(deferred, root, truth$cases[[1L]]$csv, "a deferred run keeps its own snapshot")
  assert_memory(deferred)
  empty <- frame_of(list(rows = list()))
  assert_saved(spec$run(inputs = list(SRC = empty, AUX = empty)), root,
    truth$empty_csv, "empty frames retain their schema")
  zero_path <- file.path(root, "zero-columns.yaml")
  write_text(zero_path, truth$zero_columns$specification)
  for (case in truth$zero_columns$cases) {
    frame <- data.frame(row.names = seq_len(case$rows))
    for (stream in c(FALSE, TRUE)) {
      assert_saved(yamaa_spec(zero_path)$run(stream = stream, inputs = list(SRC = frame)),
        root, case$csv, "zero-column frames retain their row count")
    }
  }
  unlink(file.path(root, "data.csv"))
  assert_saved(spec$run(inputs = list(SRC = frame_of(truth$cases[[1L]]), AUX = frame_of(truth$cases[[2L]]))),
    root, truth$all_memory_csv, "aliases sharing a missing path are independent")
  run <- spec$run(inputs = setNames(list(frame_of(truth$cases[[1L]])), truth$unknown$alias))
  assert_issue(run$issues, truth$unknown, "unknown aliases precede physical access")
  write_text(file.path(root, "data.csv"), truth$files[["data.csv"]])
  expected <- truth$missing_type
  write_text(file.path(root, "parent.yaml"), sub(expected$declaration_from, expected$declaration_to,
    truth$files[["parent.yaml"]], fixed = TRUE))
  run <- spec$run(inputs = list(SRC = frame_of(truth$cases[[1L]])))
  assert_issue(run$issues, expected, "declared types still apply")
  write_text(file.path(root, "parent.yaml"), truth$files[["parent.yaml"]])
  boolean <- frame_of(truth$cases[[1L]]); boolean$VALUE <- TRUE
  error <- refused(spec$run(inputs = list(SRC = boolean)))
  logical <- Filter(function(case) identical(case$type, "bool"), truth$unsupported_types)[[1L]]
  assert_issue(error$issues, logical$issue, "a logical column holding a value names its column and type")
  # Each kind as R holds it: an integer column with a missing value read as
  # numbers is double, and a column of NA alone is logical.
  frame_types <- truth$frame_types
  frame_types_path <- file.path(root, "frame-types.yaml")
  for (case in frame_types$cases) {
    # A case that declares its own specification runs it.
    write_text(frame_types_path, if (is.null(case$specification)) frame_types$specification else case$specification)
    columns <- lapply(case$columns, function(column) {
      values <- lapply(column$values, function(value) if (is.null(value)) NA else value)
      switch(column$kind, int = as.integer(unlist(values)), float = as.double(unlist(values)),
        str = as.character(unlist(values)), missing = as.logical(unlist(values)))
    })
    frame <- as.data.frame(setNames(columns, vapply(case$columns, `[[`, "", "name")),
      check.names = FALSE, stringsAsFactors = FALSE)
    run <- yamaa_spec(frame_types_path)$run(inputs = list(SRC = frame))
    if (!is.null(case$issue)) {
      expect_identical(run$status, "failed", case$name)
      assert_issue(run$issues, case$issue, case$name)
    } else {
      expect_identical(list(status = run$status, issues = nrow(run$issues)),
        list(status = "succeeded", issues = 0L), case$name)
      run$save()
      expect_identical(read_text(file.path(root, "frame-types.csv")), case$csv, case$name)
    }
  }
  # yamaa_read_odm() gives SourceOrdinal as a double, which the clinical-item
  # table's int refuses until it is made integer.
  round_trip <- truth$odm_round_trip
  write_text(file.path(root, "items.xml"), round_trip$document)
  round_trip_path <- file.path(root, "round-trip.yaml")
  write_text(round_trip_path, round_trip$specification)
  odm <- yamaa_read_odm(file.path(root, "items.xml"))
  run <- yamaa_spec(round_trip_path)$run(inputs = list(SRC = odm))
  expect_identical(run$status, "failed", "a read_odm table's double SourceOrdinal is not int")
  assert_issue(run$issues, round_trip$r_double_ordinal, "a read_odm table's double SourceOrdinal is not int")
  odm$SourceOrdinal <- as.integer(odm$SourceOrdinal)
  run <- yamaa_spec(round_trip_path)$run(inputs = list(SRC = odm))
  expect_identical(list(status = run$status, issues = nrow(run$issues)),
    list(status = "succeeded", issues = 0L), "an integer SourceOrdinal passes the read_odm table back")
  run$save()
  expect_identical(read_text(file.path(root, "round-trip.csv")), round_trip$csv,
    "an integer SourceOrdinal passes the read_odm table back")
  typed_path <- file.path(root, "typed.yaml"); write_text(typed_path, truth$typed$specification)
  for (factor in c(FALSE, TRUE)) {
    frame <- typed_frame()
    if (factor) frame$TEXT <- as.factor(frame$TEXT)
    run <- yamaa_spec(typed_path)$run(inputs = list(SRC = frame))
    expect_identical(run$status, "succeeded", "all supported scalar kinds build")
    run$save()
    expect_identical(read_text(file.path(root, "typed.csv")), truth$typed$csv,
      "Unicode, missing values and temporal types share the authored CSV")
  }
  for (case in truth$r_host_refusals) {
    frame <- typed_frame()
    frame[[case$column]] <- switch(case$kind,
      date = .Date(c(case$value, NA_real_)),
      datetime = .POSIXct(c(case$value, NA_real_), tz = "UTC"),
      class = structure(c(case$value, NA_real_), class = case$class))
    refused(yamaa_spec(typed_path)$run(inputs = list(SRC = frame)))
  }
  for (inputs in list(unname(list(frame_of(truth$cases[[1L]]))), list(SRC = 1), 1,
      setNames(list(frame_of(truth$cases[[1L]]), frame_of(truth$cases[[1L]])), c("SRC", "SRC")))) {
    refused(spec$run(inputs = inputs))
  }
  # A POSIXct keeps its clock time only in UTC or GMT; any other tzone,
  # including the session's, is refused rather than shifted (#4882).
  for (zone in truth$r_utc_zones) {
    frame <- typed_frame(); attr(frame$WHEN, "tzone") <- zone
    run <- yamaa_spec(typed_path)$run(inputs = list(SRC = frame))
    run$save()
    expect_identical(read_text(file.path(root, "typed.csv")), truth$typed$csv,
      paste("a POSIXct in", zone, "keeps its clock time"))
  }
  for (case in truth$r_zone_refusals) {
    frame <- typed_frame(); attr(frame$WHEN, "tzone") <- case$tzone
    error <- refused(yamaa_spec(typed_path)$run(inputs = list(SRC = frame)))
    expect_identical(conditionMessage(error), case$message,
      sprintf("a POSIXct in tzone %s is refused", paste(deparse(case$tzone), collapse = "")))
  }
  # R-only seam: byte-marked text cannot become an unchecked Rust string,
  # whether a character value or a factor level, and its refusal names it.
  bad <- rawToChar(as.raw(255L)); Encoding(bad) <- "bytes"
  for (case in truth$r_text_refusals) {
    invalid <- typed_frame()
    invalid$TEXT <- switch(case$kind, character = c("ok", bad),
      factor = structure(1:2, levels = c("ok", bad), class = "factor"))
    error <- refused(yamaa_spec(typed_path)$run(inputs = list(SRC = invalid)))
    expect_identical(conditionMessage(error), case$message,
      paste("a byte-marked", case$kind, "value is refused by name"))
  }
  # A factor level no row uses is never read, so it cannot refuse the column.
  unused <- typed_frame()
  unused$TEXT <- factor(unused$TEXT, levels = c(unused$TEXT[[1L]], bad))
  run <- yamaa_spec(typed_path)$run(inputs = list(SRC = unused))
  run$save()
  expect_identical(read_text(file.path(root, "typed.csv")), truth$typed$csv,
    "an unused byte-marked factor level is not read")
  cat("memory input snapshots, scalar types, provenance and refusals pass\n")
}
qualify_memory_inputs()
