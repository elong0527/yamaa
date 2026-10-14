# Installed 1,024/1,025 controls for every width route (#2299).
library(yamaa)
stopifnot(requireNamespace("arrow", quietly = TRUE),
          requireNamespace("nanoarrow", quietly = TRUE))

write_case <- function(root, columns = 1L, fields = 1L, typed = FALSE,
                       source_format = "csv", selected = NULL) {
  dir.create(root, recursive = TRUE, showWarnings = FALSE)
  names <- c("ID", sprintf("C%04d", seq_len(columns - 1L)))
  header <- c("ID", sprintf("F%04d", seq_len(fields - 1L)))
  values <- as.data.frame(setNames(rep(list("x"), fields), header),
                          stringsAsFactors = FALSE, check.names = FALSE)
  values$ID <- "S-001"
  source <- paste0("input.", source_format)
  if (source_format == "parquet") {
    arrow::write_parquet(values, file.path(root, source))
  } else {
    writeLines(c(paste(header, collapse = ","),
                 paste(unlist(values, use.names = FALSE), collapse = ",")),
               file.path(root, source))
  }
  if (typed) {
    types <- paste(sprintf("%s: str", header), collapse = ", ")
    source <- sprintf("{path: %s, types: {%s}}", source, types)
  }
  column_lines <- c(
    "  - {name: ID, type: str, label: ID, derivation: SRC.ID}",
    sprintf("  - {name: %s, type: str, label: Column, derivation: {literal: x}}", names[-1L])
  )
  if (is.null(selected)) selected <- names
  spec <- c(
    "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    sprintf("input: {SRC: %s}", source), "columns:", column_lines,
    sprintf("output: {path: out.csv, columns: [%s]}", paste(selected, collapse = ", "))
  )
  path <- file.path(root, "spec.yaml")
  writeLines(spec, path)
  path
}

assert_build <- function(spec, width = 1L) {
  stopifnot(nrow(yamaa_check(spec)$issues) == 0L)
  built <- yamaa_domain(spec)
  stopifnot(nrow(built$issues) == 0L, nrow(built$output) == 1L,
            ncol(built$output) == width, identical(built$output$ID, "S-001"))
  if (width > 1L) stopifnot(all(built$output[1L, -1L] == "x"))
}

assert_refusal <- function(result, compiler = FALSE) {
  issues <- result$issues
  stopifnot(nrow(issues) > 0L, any(issues$condition == "engine_rejected"))
  if (compiler) stopifnot(any(grepl('"code":"compiler_limit"', issues$context, fixed = TRUE)))
  if (is.environment(result)) {
    stopifnot(is.null(result$output),
              inherits(tryCatch(result$save(), error = identity), "yamaa_domain_error"))
  }
}

run_column_ceilings <- function() {
  root <- tempfile("yamaa-column-ceilings-")
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)

  # Compiler capacity facts are identical through both operations and routes.
  for (resource in c("columns", "inputs", "source_fields", "projected_columns")) {
    case_root <- path(paste0("details-", resource))
    actual <- if (resource == "inputs") 17L else 1025L
    limit <- if (resource == "inputs") 16L else 1024L
    spec <- write_case(case_root,
      columns = if (resource == "columns") actual else 1L,
      fields = if (resource == "source_fields") actual else 1L,
      typed = resource == "source_fields",
      selected = if (resource == "projected_columns") rep("ID", actual) else "ID")
    if (resource == "inputs") {
      lines <- readLines(spec)
      lines[startsWith(lines, "input:")] <- sprintf("input: {%s}",
        paste(sprintf("S%s: input.csv", seq_len(actual)), collapse = ", "))
      writeLines(lines, spec)
    }
    env <- file.path(case_root, "environment.yaml")
    writeLines(c("schema_version: '1.0'", "language: r"), env)
    spec_path <- switch(resource, inputs = "input", source_fields = "input.SRC.types",
                        projected_columns = "output.columns", "columns")
    expected <- sprintf('{"actual":%s,"code":"compiler_limit","limit":%s,"resource":"%s","stage":"prepare"}',
                        actual, limit, resource)
    for (environment in list(NULL, env)) {
      for (operation in list(yamaa_check, yamaa_domain)) {
        issue <- operation(spec, environment = environment)$issues
        stopifnot(nrow(issue) == 1L, identical(issue$phase, "engine"),
          identical(issue$condition, "engine_rejected"), is.na(issue$requirement),
          identical(issue$spec_paths, list(spec_path)), identical(issue$context, expected))
      }
    }
  }

  # Narrow output isolates the declared-column ceiling from projection width.
  assert_build(write_case(path("admit-columns"), 1024L, selected = "ID"))
  spec <- write_case(path("refuse-columns"), 1025L, selected = "ID")
  for (operation in list(yamaa_check, yamaa_domain)) assert_refusal(operation(spec), TRUE)

  assert_build(write_case(path("admit-output"), 1024L), 1024L)
  names <- c("ID", sprintf("C%04d", seq_len(1023L)))
  # Keep declarations in range: projection count refuses before the repeated
  # output name is validated, independently of the declared-column cap.
  spec <- write_case(path("refuse-output"), 1024L, selected = c(names, "ID"))
  for (operation in list(yamaa_check, yamaa_domain)) assert_refusal(operation(spec), TRUE)

  assert_build(write_case(path("admit-types"), fields = 1024L, typed = TRUE))
  spec <- write_case(path("refuse-types"), fields = 1025L, typed = TRUE)
  for (operation in list(yamaa_check, yamaa_domain)) assert_refusal(operation(spec), TRUE)

  for (format in c("csv", "parquet")) {
    assert_build(write_case(path(paste0("admit-", format)), fields = 1024L, source_format = format))
    spec <- write_case(path(paste0("refuse-", format)), fields = 1025L, source_format = format)
    assert_refusal(yamaa_domain(spec))
  }

  for (width in c(1024L, 1025L)) {
    producer <- write_case(path(paste0("producer-", width)), columns = width)
    consumer <- file.path(dirname(producer), "consumer.yaml")
    writeLines(c(
      "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
      "input: {SRC: {path: out.csv, schema: spec.yaml}}", "columns:",
      "  - {name: ID, type: str, label: ID, derivation: SRC.ID}",
      "output: {path: consumer.csv, columns: [ID]}"
    ), consumer)
    if (width == 1024L) {
      assert_build(consumer)
    } else {
      for (operation in list(yamaa_check, yamaa_domain)) assert_refusal(operation(consumer))
    }
  }

  # Keep producer metadata in range while its raw source crosses the runtime
  # width ceiling; the Rust decoder control pins producer_source_fields itself.
  for (width in c(1024L, 1025L)) {
    producer <- write_case(path(paste0("producer-input-", width)), columns = 1024L, fields = width)
    consumer <- file.path(dirname(producer), "consumer.yaml")
    writeLines(c(
      "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
      "input: {SRC: {path: out.csv, schema: spec.yaml}}", "columns:",
      "  - {name: ID, type: str, label: ID, derivation: SRC.ID}",
      "output: {path: consumer.csv, columns: [ID]}"
    ), consumer)
    stopifnot(nrow(yamaa_check(consumer)$issues) == 0L)
    if (width == 1024L) assert_build(consumer) else assert_refusal(yamaa_domain(consumer))
  }

  # Read an actual IPC source through the installed C Stream entry point.
  no_io <- function(...) stop("in-memory execution entered file IO")
  prepare <- get(".prepare_document", asNamespace("yamaa"))
  execute <- get(".execute_specification_arrow", asNamespace("yamaa"))
  spec <- write_case(path("ipc"))
  source <- readBin(spec, "raw", n = file.info(spec)$size)
  run <- prepare("memory.yaml", source, no_io, no_io, no_io)
  header <- c("ID", sprintf("F%04d", seq_len(1023L)))
  values <- as.data.frame(setNames(rep(list("x"), 1024L), header),
                          stringsAsFactors = FALSE, check.names = FALSE)
  values$ID <- "S-001"
  ipc <- file.path(root, "input.arrow")
  arrow::write_ipc_stream(values, ipc)
  assert_ipc_build <- function() {
    result <- execute(run, arrow::read_ipc_stream(ipc, as_data_frame = FALSE))
    stopifnot(grepl('"status":"success"', result$outcome, fixed = TRUE))
    output <- arrow::read_ipc_stream(arrow::BufferReader$create(result$table))
    stopifnot(identical(names(output), "ID"), identical(output$ID, "S-001"))
  }
  assert_ipc_build()
  values$EXCESS <- "x"
  released <- 0L
  overwide <- nanoarrow::array_stream_set_finalizer(
    nanoarrow::basic_array_stream(list(values)), function() released <<- released + 1L)
  failure <- tryCatch(execute(run, overwide), error = identity)
  stopifnot(inherits(failure, "error"),
            grepl("resource limit", conditionMessage(failure), fixed = TRUE),
            released == 1L, !nanoarrow::nanoarrow_pointer_is_valid(overwide))
  assert_ipc_build()
}

run_column_ceilings()
