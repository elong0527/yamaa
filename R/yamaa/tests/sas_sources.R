# Installed public SAS input controls with independently authored typed truth.
library(yamaa)
host_fixture_root <- if (file.exists("host-fixtures/sas-cases.json")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
truth <- jsonlite::fromJSON(file.path(host_fixture_root, "sas-cases.json"), simplifyVector = FALSE)
stopifnot(identical(truth$version, "1"))

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
  if (!is.null(mutation$set_byte)) {
    payload[mutation$set_byte[[1L]] + 1L] <- as.raw(mutation$set_byte[[2L]])
  }
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
    if (field$type == "int") stopifnot(identical(class(value), "yamaa_int64_vector"))
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
    if (field$type == "int") value <- as.character(value)
    if (field$type == "date") value <- format(value, "%Y-%m-%d")
    if (field$type == "datetime") value <- format(value, "%Y-%m-%dT%H:%M:%S", tz = "UTC", usetz = FALSE)
    value
  })
  lapply(seq_len(nrow(output)), function(i) {
    lapply(columns, function(value) if (is.na(value[[i]])) NULL else unname(value[[i]]))
  })
}
assert_output <- function(result, case) {
  stopifnot(nrow(result$issues) == 0L, !is.null(result$output))
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
    stopifnot(inherits(tryCatch(result$save(), error = identity), "yamaa_domain_error"),
      identical(result$issues, before), !file.exists(file.path(root, "result.csv")))
  }
}

run_sas_sources <- function() {
  root <- tempfile("yamaa-sas-sources-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  for (case in truth$accepted) {
    paths <- stage_case(root, case)
    stopifnot(nrow(yamaa_check(paths$spec)$issues) == 0L,
      !file.exists(file.path(paths$root, "result.csv")))
    result <- yamaa_domain(paths$spec)
    assert_output(result, case)
    stopifnot(identical(result$save(), TRUE))
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
    stopifnot(nrow(yamaa_check(paths$spec)$issues) == 0L)
    result <- yamaa_domain(paths$spec)
    assert_output(result, case)
    stopifnot(identical(result$save(), TRUE), file.exists(file.path(paths$root, "result.csv")))
  }

  for (case in truth$static_refusals) {
    paths <- stage_case(root, case)
    checked <- yamaa_check(paths$spec)
    result <- yamaa_domain(paths$spec)
    stopifnot(identical(checked$issues, result$issues))
    assert_failed(result, case, paths$root)
  }
  for (case in truth$refusals) {
    paths <- stage_case(root, case)
    stopifnot(nrow(yamaa_check(paths$spec)$issues) == 0L,
      !file.exists(file.path(paths$root, "result.csv")))
    assert_failed(yamaa_domain(paths$spec), case, paths$root)
  }

  case <- truth$accepted[[which(vapply(truth$accepted, `[[`, character(1L), "id") == truth$snapshot_case)]]
  case$id <- "held-snapshot"
  paths <- stage_case(root, case)
  result <- yamaa_domain(paths$spec)
  assert_output(result, case)
  held <- result$output
  writeBin(charToRaw("changed source"), paths$source)
  assert_failed(yamaa_domain(paths$spec),
    list(condition = "source_sas_invalid", phase = "ingest", causes = list("framing")), paths$root)
  unlink(c(paths$source, paths$spec))
  gc()
  for (i in seq_len(2L)) {
    stopifnot(identical(result$save(), TRUE), identical(result$output, held),
      identical(read_bytes(file.path(paths$root, "result.csv")), charToRaw(enc2utf8(case$csv))))
  }
  rm(result)
  gc()
  stopifnot(identical(table_rows(held, case$fields), case$rows))
}

run_sas_sources()
cat("installed R SAS source, narrowing, encoding and retained-publication controls passed\n")
