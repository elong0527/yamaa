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
  assert_issue(error$issues, truth$unsupported_type, "logical columns are refused without a cast")
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
  # R-only seam: byte-marked text cannot become an unchecked Rust string.
  invalid <- typed_frame(); invalid$TEXT <- rep(rawToChar(as.raw(255L)), 2L)
  Encoding(invalid$TEXT) <- "bytes"
  refused(yamaa_spec(typed_path)$run(inputs = list(SRC = invalid)))
  cat("memory input snapshots, scalar types, provenance and refusals pass\n")
}
qualify_memory_inputs()
