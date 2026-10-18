# Installed public SAS input controls with independently authored typed truth.
library(yamaa)
host_fixture_root <- if (file.exists("host-fixtures/sas-cases.json")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
truth <- jsonlite::fromJSON(file.path(host_fixture_root, "sas-cases.json"), simplifyVector = FALSE)
stopifnot(identical(truth$version, "1"))
check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

read_bytes <- function(path) readBin(path, "raw", n = file.info(path)$size)
hex_bytes <- function(value) {
  if (!nzchar(value)) return(raw())
  starts <- seq.int(1L, nchar(value), by = 2L)
  as.raw(strtoi(substring(value, starts, starts + 1L), base = 16L))
}
stage_case <- function(root, case) {
  directory <- file.path(root, case$id)
  dir.create(directory)
  payload <- read_bytes(file.path(host_fixture_root, "sas", case$file))
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
    stopifnot(length(matched) == 1L, length(before) == length(after))
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
assert_types <- function(output, fields) {
  stopifnot(identical(names(output), field_names(fields)))
  for (field in fields) {
    value <- output[[field$name]]
    if (field$type == "int") stopifnot(is.double(value), is.null(attributes(value)))
    if (field$type == "float") stopifnot(is.double(value), is.null(attributes(value)))
    if (field$type == "str") stopifnot(is.character(value), is.null(attributes(value)))
    if (field$type == "date") stopifnot(identical(class(value), "Date"))
    if (field$type == "datetime") {
      stopifnot(identical(class(value), c("POSIXct", "POSIXt")), identical(attr(value, "tzone"), "UTC"))
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
  stopifnot(nrow(result$issues) == 0L)
  unheld <- unheld_integer(case)
  if (!is.null(unheld)) {
    # R exchanges int as double: the successful run's output view refuses a
    # value beyond 2^53 rather than change it. Its saved bytes stay exact.
    failure <- tryCatch(result$output, error = identity)
    stopifnot(inherits(failure, "yamaa_error"), identical(conditionMessage(failure),
      paste0("integer ", unheld, " is beyond 2^53 and cannot be held exactly by an R double")),
      nrow(result$issues) == 0L)
    return(invisible())
  }
  stopifnot(!is.null(result$output))
  assert_types(result$output, case$fields)
  rows <- table_rows(result$output, case$fields)
  if (!is.null(case$rows)) {
    stopifnot(identical(rows, case$rows))
  } else {
    # The independent haven test states only its first row. Keep this owner
    # rather than generating new expected rows with the candidate parser.
    stopifnot(identical(rows[[1L]], case$first_row),
      identical(vapply(rows, `[[`, character(1L), 1L), as.character(seq_along(rows))))
  }
}
assert_failed <- function(result, case, root) {
  before <- result$issues
  phase <- if (is.null(case$phase)) "validation" else case$phase
  stopifnot(identical(before$condition, case$condition), identical(before$phase, phase),
    is.null(result$output))
  if (!is.null(case$context_fields)) {
    context <- jsonlite::fromJSON(before$context[[1L]], simplifyVector = FALSE)
    for (name in names(case$context_fields)) stopifnot(identical(context[[name]], case$context_fields[[name]]))
    stopifnot(identical(before$spec_paths[[1L]], unlist(case$spec_paths, use.names = FALSE)))
  }
  if (!is.null(case$causes)) {
    context <- jsonlite::fromJSON(before$context[[1L]], simplifyVector = FALSE)
    stopifnot(identical(context$dataset, "SRC"), context$cause %in% unlist(case$causes),
      identical(before$spec_paths[[1L]], "input.SRC.path"))
  }
  for (i in seq_len(2L)) {
    # A failed run saves nothing and signals the run's unchanged issues.
    failure <- tryCatch(result$save(), error = identity)
    stopifnot(inherits(failure, "yamaa_error"),
      identical(conditionMessage(failure), "cannot save a failed run; see issues"),
      identical(failure$issues, before),
      identical(result$issues, before), !file.exists(file.path(root, "result.csv")))
  }
}

run_sas_sources <- function() {
  root <- tempfile("yamaa-sas-sources-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  for (case in truth$accepted) {
    paths <- stage_case(root, case)
    stopifnot(nrow(check(paths$spec)$issues) == 0L,
      !file.exists(file.path(paths$root, "result.csv")))
    result <- run(paths$spec)
    assert_output(result, case)
    stopifnot(is.null(result$save()))
    if (!is.null(case$csv)) stopifnot(identical(read_bytes(file.path(paths$root, "result.csv")), charToRaw(enc2utf8(case$csv))))
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
    document <- list(schema_version = "1.0", domain = "TEST", base = "SRC", keys = list("RECORD"),
      input = list(SRC = list(path = file, encoding = "windows-1252", ordinal = "RECORD")),
      columns = lapply(fields, function(field) c(field, list(label = field$name, derivation = paste0("SRC.", field$name)))),
      output = list(path = "result.csv", columns = as.list(field_names(fields))))
    case <- list(id = file, file = file, specification = document, fields = fields, rows = expected_rows)
    paths <- stage_case(root, case)
    stopifnot(nrow(check(paths$spec)$issues) == 0L)
    result <- run(paths$spec)
    assert_output(result, case)
    stopifnot(is.null(result$save()), file.exists(file.path(paths$root, "result.csv")))
  }

  for (case in truth$static_refusals) {
    paths <- stage_case(root, case)
    checked <- check(paths$spec)
    result <- run(paths$spec)
    stopifnot(identical(checked$issues, result$issues))
    assert_failed(result, case, paths$root)
  }
  for (case in truth$refusals) {
    paths <- stage_case(root, case)
    stopifnot(nrow(check(paths$spec)$issues) == 0L,
      !file.exists(file.path(paths$root, "result.csv")))
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
    stopifnot(is.null(result$save()), identical(result$output, held),
      identical(read_bytes(file.path(paths$root, "result.csv")), charToRaw(enc2utf8(case$csv))))
  }
  rm(result)
  gc()
  stopifnot(identical(table_rows(held, case$fields), case$rows))
}

run_sas_sources()
cat("installed R SAS source, narrowing, encoding and retained-publication controls passed\n")
