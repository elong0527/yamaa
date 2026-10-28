# Installed public SAS input controls with independently authored typed truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped sas_sources.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- jsonlite::fromJSON(file.path(host_fixture_root, "sas-cases.json"), simplifyVector = FALSE)
expect_identical(truth$version, "1", "the SAS truth's version")
check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

# The bytes that authored hexadecimal text spells.
hex_bytes <- function(value) fixture_bytes(list(hex = value))
stage_case <- function(root, case) {
  directory <- file.path(root, case$id)
  dir.create(directory)
  payload <- rawfile(file.path(host_fixture_root, "sas", case$file))
  mutation <- case$mutation
  if (!is.null(mutation$bytes_hex)) payload <- hex_bytes(mutation$bytes_hex)
  if (!is.null(mutation$truncate)) {
    length <- mutation$truncate
    if (length < 0L) length <- base::length(payload) + length
    payload <- payload[seq_len(length)]
  }
  if (!is.null(mutation$replace_hex)) {
    before <- hex_bytes(mutation$replace_hex[[1L]])
    after <- hex_bytes(mutation$replace_hex[[2L]])
    starts <- seq_len(length(payload) - length(before) + 1L)
    matched <- starts[vapply(starts, function(i) identical(payload[i:(i + length(before) - 1L)], before), logical(1L))]
    stopifnot_case(case$id, "the replaced bytes occur once" = length(matched) == 1L,
      "the replacement keeps the length" = length(before) == length(after))
    payload[matched:(matched + length(before) - 1L)] <- after
  }
  # `[[` matches exactly; `$` would also match set_bytes for set_byte.
  changes <- c(if (!is.null(mutation[["set_byte"]])) list(mutation[["set_byte"]]), mutation[["set_bytes"]])
  for (change in changes) payload[change[[1L]] + 1L] <- as.raw(change[[2L]])
  source <- file.path(directory, case$file)
  writeBin(payload, source)
  spec <- file.path(directory, "spec.yaml")
  document <- jsonlite::toJSON(case$specification, auto_unbox = TRUE, null = "null", digits = NA)
  writeBin(charToRaw(enc2utf8(document)), spec)
  list(root = directory, spec = spec, source = source)
}
field_names <- function(fields) vapply(fields, `[[`, character(1L), "name")
assert_types <- function(output, fields, label) {
  expect_identical(names(output), field_names(fields), paste(label, "has the declared columns"))
  for (field in fields) {
    value <- output[[field$name]]
    where <- sprintf("%s column %s (%s)", label, field$name, field$type)
    if (field$type %in% c("int", "float"))
      stopifnot_case(where, "a number is a plain double" = is.double(value) && is.null(attributes(value)))
    if (field$type == "str")
      stopifnot_case(where, "text is plain character" = is.character(value) && is.null(attributes(value)))
    if (field$type == "date") expect_identical(class(value), "Date", paste(where, "is Date"))
    if (field$type == "datetime") {
      expect_identical(class(value), c("POSIXct", "POSIXt"), paste(where, "is POSIXct"))
      expect_identical(attr(value, "tzone"), "UTC", paste(where, "is in UTC"))
    }
  }
}
table_rows <- function(output, fields) {
  columns <- lapply(fields, function(field) {
    value <- output[[field$name]]
    if (field$type == "int") value <- ifelse(is.na(value), NA_character_, sprintf("%.0f", value))
    if (field$type == "date") value <- format(value, "%Y-%m-%d")
    if (field$type == "datetime") value <- format(value, "%Y-%m-%dT%H:%M:%S", tz = "UTC", usetz = FALSE)
    value
  })
  lapply(seq_len(nrow(output)), function(i) {
    lapply(columns, function(value) if (is.na(value[[i]])) NULL else unname(value[[i]]))
  })
}
# Decimal integer text beyond 2^53 in magnitude, compared as exact digits.
beyond_double <- function(text) {
  digits <- sub("^-", "", text)
  nchar(digits) > 16L || (nchar(digits) == 16L && digits > "9007199254740992")
}
# The first authored int cell an R double cannot hold, column by column.
unheld_integer <- function(case) {
  if (is.null(case$rows)) return(NULL)
  for (index in seq_along(case$fields)) if (case$fields[[index]]$type == "int") {
    for (row in case$rows) {
      cell <- row[[index]]
      if (is.character(cell) && beyond_double(cell)) return(cell)
    }
  }
  NULL
}
assert_output <- function(result, case) {
  label <- paste("SAS case", case$id)
  stopifnot_case(label, "the run has no issues" = nrow(result$issues) == 0L)
  unheld <- unheld_integer(case)
  if (!is.null(unheld)) {
    # R exchanges int as double: the successful run's output view refuses a
    # value beyond 2^53 rather than change it. Its saved bytes stay exact.
    failure <- tryCatch(result$output, error = identity)
    stopifnot_case(label, "the output view signals yamaa_error" = inherits(failure, "yamaa_error"),
      "reading the view adds no issue" = nrow(result$issues) == 0L)
    expect_identical(conditionMessage(failure),
      paste0("integer ", unheld, " is beyond 2^53 and cannot be held exactly by an R double"),
      paste(label, "refuses the first unheld integer"))
    return(invisible())
  }
  stopifnot_case(label, "the run has output" = !is.null(result$output))
  assert_types(result$output, case$fields, label)
  rows <- table_rows(result$output, case$fields)
  if (!is.null(case$rows)) {
    expect_identical(rows, case$rows, paste(label, "has the authored rows"))
  } else {
    # The independent haven test states only its first row. Keep this owner
    # rather than generating new expected rows with the candidate parser.
    expect_identical(rows[[1L]], case$first_row, paste(label, "has the authored first row"))
    expect_identical(vapply(rows, `[[`, character(1L), 1L), as.character(seq_along(rows)),
      paste(label, "numbers its records in order"))
  }
}
assert_failed <- function(result, case, root) {
  label <- paste("SAS refusal", if (is.null(case$id)) case$condition else case$id)
  before <- result$issues
  phase <- if (is.null(case$phase)) "validation" else case$phase
  expect_identical(as.list(before[c("condition", "phase")]), list(condition = case$condition, phase = phase),
    paste(label, "is the authored refusal"))
  stopifnot_case(label, "a refused run has no output" = is.null(result$output))
  if (!is.null(case$context_fields)) {
    context <- jsonlite::fromJSON(before$context[[1L]], simplifyVector = FALSE)
    expect_identical(context[names(case$context_fields)], case$context_fields, paste(label, "has the authored context"))
    expect_identical(before$spec_paths[[1L]], unlist(case$spec_paths, use.names = FALSE),
      paste(label, "names the authored spec paths"))
  }
  if (!is.null(case$causes)) {
    context <- jsonlite::fromJSON(before$context[[1L]], simplifyVector = FALSE)
    stopifnot_case(label, "the context names SRC" = identical(context$dataset, "SRC"),
      "the cause is an authored one" = context$cause %in% unlist(case$causes),
      "the issue is at the source path" = identical(before$spec_paths[[1L]], "input.SRC.path"))
  }
  for (i in seq_len(2L)) {
    # A failed run saves nothing and signals the run's unchanged issues.
    failure <- tryCatch(result$save(), error = identity)
    stopifnot_case(sprintf("%s, save %d", label, i),
      "the save signals yamaa_error with D39's text" = inherits(failure, "yamaa_error") && save_refusal_text(failure, before),
      "it carries the run's issues" = identical(failure$issues, before),
      "the run's issues stay" = identical(result$issues, before),
      "nothing is written" = !file.exists(file.path(root, "result.csv")))
  }
}

run_sas_sources <- function() {
  root <- tempfile("yamaa-sas-sources-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  for (case in truth$accepted) {
    paths <- stage_case(root, case)
    stopifnot_case(case$id, "the case checks" = nrow(check(paths$spec)$issues) == 0L,
      "a check writes nothing" = !file.exists(file.path(paths$root, "result.csv")))
    result <- run(paths$spec)
    assert_output(result, case)
    stopifnot_case(case$id, "the run saves" = returns_paths(result$save()))
    if (!is.null(case$csv)) expect_identical(rawfile(file.path(paths$root, "result.csv")), charToRaw(enc2utf8(case$csv)),
      paste(case$id, "saves the authored CSV"))
  }

  # Upstream pandas supplies independent truth for every compressed-source cell.
  owner <- truth$compressed
  expected <- read.csv(file.path(host_fixture_root, "sas", owner$truth),
    colClasses = "character", check.names = FALSE, na.strings = character(),
    stringsAsFactors = FALSE, fileEncoding = "UTF-8")
  columns <- names(expected)
  text_columns <- unlist(owner$text_columns, use.names = FALSE)
  fields <- c(list(list(name = "RECORD", type = "int")), lapply(columns, function(name) {
    list(name = name, type = if (name %in% text_columns) "str" else "float")
  }))
  expected_rows <- lapply(seq_len(nrow(expected)), function(i) {
    c(list(as.character(i)), lapply(columns, function(name) {
      value <- expected[[name]][[i]]
      if (!nzchar(value)) NULL else if (name %in% text_columns) value else as.double(value)
    }))
  })
  for (file in owner$files) {
    document <- list(yamaa = "1.0", type = "spec", domain = "TEST", base = "SRC", keys = list("RECORD"),
      input = list(SRC = list(path = file, encoding = owner$encodings[[file]], ordinal = "RECORD")),
      columns = lapply(fields, function(field) c(field, list(label = field$name, derivation = paste0("SRC.", field$name)))),
      output = list(path = "result.csv", columns = as.list(field_names(fields))))
    case <- list(id = file, file = file, specification = document, fields = fields, rows = expected_rows)
    paths <- stage_case(root, case)
    stopifnot_case(file, "the compressed case checks" = nrow(check(paths$spec)$issues) == 0L)
    result <- run(paths$spec)
    assert_output(result, case)
    stopifnot_case(file, "the run saves" = returns_paths(result$save()),
      "it writes the CSV" = file.exists(file.path(paths$root, "result.csv")))
  }

  for (case in truth$static_refusals) {
    paths <- stage_case(root, case)
    checked <- check(paths$spec)
    result <- run(paths$spec)
    expect_identical(checked$issues, result$issues, paste(case$id, "is refused by the check as by the run"))
    assert_failed(result, case, paths$root)
  }
  for (case in truth$refusals) {
    paths <- stage_case(root, case)
    stopifnot_case(case$id, "the case checks" = nrow(check(paths$spec)$issues) == 0L,
      "a check writes nothing" = !file.exists(file.path(paths$root, "result.csv")))
    assert_failed(run(paths$spec), case, paths$root)
  }

  case <- truth$accepted[[which(vapply(truth$accepted, `[[`, character(1L), "id") == truth$snapshot_case)]]
  case$id <- "held-snapshot"
  paths <- stage_case(root, case)
  result <- run(paths$spec)
  assert_output(result, case)
  held <- result$output
  writeBin(charToRaw("changed source"), paths$source)
  assert_failed(run(paths$spec),
    list(condition = "source_sas_invalid", phase = "ingest", causes = list("framing")), paths$root)
  unlink(c(paths$source, paths$spec))
  gc()
  for (i in seq_len(2L)) {
    stopifnot_case(sprintf("held save %d", i), "the held run saves" = returns_paths(result$save()))
    expect_identical(result$output, held, sprintf("held save %d leaves the output", i))
    expect_identical(rawfile(file.path(paths$root, "result.csv")), charToRaw(enc2utf8(case$csv)),
      sprintf("held save %d writes the authored CSV", i))
  }
  rm(result)
  gc()
  expect_identical(table_rows(held, case$fields), case$rows, "a held output outlives its run")
}

run_sas_sources()
cat("installed R SAS source, narrowing, encoding and retained-publication controls passed\n")
