# Installed 1,024/1,025 controls for every width route (#2299).
library(yamaa)
stopifnot(requireNamespace("arrow", quietly = TRUE))

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

assert_layered_cases <- function(root, width) {
  dir.create(root, recursive = TRUE)
  writeLines(c("ID,A,B", "S-001,x,1", "S-002,y,2", "S-003,,3"),
             file.path(root, "input.csv"))
  names <- c("ID", sprintf("K%04d", seq_len(width - 1L)))
  case_column <- function(index) c(
    sprintf("  - name: K%04d", index), "    type: str", "    derivation:", "      case:",
    sprintf("        - when: \"SRC.A = 'x' AND SRC.B = '%d'\"", index %% 3L),
    "          then: {literal: P}", "        - when: \"SRC.A IS NULL\"",
    "          then: {literal: Q}", "        - otherwise: SRC.A")
  document <- c(
    "schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    unlist(lapply(seq_len(width - 1L), case_column), use.names = FALSE),
    sprintf("output: {path: out.csv, columns: [%s]}", paste(names, collapse = ", ")))
  writeLines(document, file.path(root, "parent.yaml"))
  writeLines(c(document, "parents: []"), file.path(root, "explicit.yaml"))
  writeLines(c("schema_version: '1.0'", "parents: [parent.yaml]"),
             file.path(root, "child.yaml"))
  writeLines(c("schema_version: '1.0'", "parents: [child.yaml]"),
             file.path(root, "grandchild.yaml"))
  rows <- list(
    c("S-001", ifelse(seq_len(width - 1L) %% 3L == 1L, "P", "x")),
    c("S-002", rep("y", width - 1L)), c("S-003", rep("Q", width - 1L)))
  expected <- charToRaw(paste0(paste(c(paste(names, collapse = ","),
    vapply(rows, paste, character(1L), collapse = ",")), collapse = "\n"), "\n"))
  for (file in c("parent.yaml", "explicit.yaml", "child.yaml", "grandchild.yaml")) {
    spec <- file.path(root, file)
    stopifnot(nrow(yamaa_check(spec)$issues) == 0L)
    built <- yamaa_domain(spec)
    stopifnot(nrow(built$issues) == 0L, identical(names(built$output), names),
              nrow(built$output) == 3L, ncol(built$output) == width)
    for (row in seq_along(rows)) {
      stopifnot(identical(unlist(built$output[row, ], use.names = FALSE), rows[[row]]))
    }
    built$save()
    output <- file.path(root, "out.csv")
    stopifnot(identical(readBin(output, "raw", n = file.info(output)$size), expected))
  }
}

run_column_ceilings <- function() {
  root <- tempfile("yamaa-column-ceilings-")
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)

  for (width in c(40L, 64L, 1024L)) {
    assert_layered_cases(path(paste0("layered-", width)), width)
  }
  parent <- write_case(path("repeated-parents"))
  child <- file.path(dirname(parent), "child.yaml")
  writeLines(c("schema_version: '1.0'",
               paste0("parents: [", paste(rep("spec.yaml", 8193L), collapse = ", "), "]")), child)
  for (operation in list(yamaa_check, yamaa_domain)) {
    result <- operation(child)
    assert_refusal(result)
    contexts <- result$issues$context
    with_resource <- grepl('"resource":', contexts, fixed = TRUE)
    if (any(with_resource)) {
      details <- contexts[with_resource]
      visits <- grepl('"resource":"parent_visits"', details, fixed = TRUE) &
        grepl('"limit":8192', details, fixed = TRUE)
      paths <- grepl('"resource":"path_bytes"', details, fixed = TRUE) &
        grepl('"limit":1048576', details, fixed = TRUE)
      stopifnot(all(visits | paths))
    }
  }

  # A quota refusal does not certify that later semantic checks passed. Once
  # capacity is repaired, both operations and routes expose the remaining error.
  case_root <- path("incomplete-checks")
  spec <- write_case(case_root, columns = 1025L, selected = "ID")
  lines <- readLines(spec)
  lines[grepl("name: C0001,", lines, fixed = TRUE)] <-
    "  - {name: C0001, type: int, derivation: {literal: 1}}"
  lines[grepl("name: C0002,", lines, fixed = TRUE)] <-
    "  - {name: C0002, type: str, derivation: {str_case: {source: C0001, to: upper}}}"
  env <- file.path(case_root, "environment.yaml")
  writeLines(c("schema_version: '1.0'", "language: r"), env)
  for (environment in list(NULL, env)) {
    writeLines(lines, spec)
    for (operation in list(yamaa_check, yamaa_domain)) {
      refused <- operation(spec, environment = environment)
      assert_refusal(refused, TRUE)
      issue <- refused$issues
      stopifnot(nrow(issue) == 1L, is.na(issue$requirement),
        grepl('"incomplete_checks_from":"specification"', issue$context, fixed = TRUE),
        grepl('"actual":1025', issue$context, fixed = TRUE),
        grepl('"limit":1024', issue$context, fixed = TRUE))
    }
    writeLines(lines[!grepl("name: C1024,", lines, fixed = TRUE)], spec)
    for (operation in list(yamaa_check, yamaa_domain)) {
      issue <- operation(spec, environment = environment)$issues
      stopifnot(nrow(issue) == 1L, identical(issue$condition, "incompatible_input_type"),
        identical(issue$requirement, "REQ-0308"),
        !grepl("incomplete_checks_from", issue$context, fixed = TRUE))
    }
    stopifnot(!file.exists(file.path(case_root, "out.csv")))
  }

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
    expected <- sprintf('{"actual":%s,"code":"compiler_limit","incomplete_checks_from":"specification","limit":%s,"resource":"%s","stage":"prepare"}',
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

  # Layered traversal and strict YAML decoding retain their original phase and bound.
  for (resource in c("depth", "numeric_digits")) {
    case_root <- path(paste0("preparation-", resource))
    spec <- write_case(case_root)
    if (resource == "depth") {
      file.copy(spec, file.path(case_root, "base.yaml"))
      for (index in 0:64) {
        parent <- if (index < 64L) sprintf("layer-%s.yaml", index + 1L) else "base.yaml"
        writeLines(c("schema_version: '1.0'", sprintf("parents: [%s]", parent)),
                   file.path(case_root, sprintf("layer-%s.yaml", index)))
      }
      writeLines(c("schema_version: '1.0'", "parents: [layer-0.yaml]"), spec)
      expected <- '{"code":"resource_limit","incomplete_checks_from":"specification","limit":64,"phase":"inheritance","resource":"depth","stage":"prepare"}'
    } else {
      writeLines(paste0("value: ", paste(rep("9", 4097L), collapse = "")), spec)
      expected <- '{"actual":4097,"code":"resource_limit","incomplete_checks_from":"specification","limit":4096,"phase":"yaml_decode","resource":"numeric_digits","stage":"capture"}'
    }
    env <- file.path(case_root, "environment.yaml")
    writeLines(c("schema_version: '1.0'", "language: r"), env)
    for (environment in list(NULL, env)) {
      for (operation in list(yamaa_check, yamaa_domain)) {
        issue <- operation(spec, environment = environment)$issues
        stopifnot(nrow(issue) == 1L, identical(issue$phase, "engine"),
          identical(issue$condition, "engine_rejected"), is.na(issue$requirement),
          identical(issue$context, expected))
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
}

run_column_ceilings()
