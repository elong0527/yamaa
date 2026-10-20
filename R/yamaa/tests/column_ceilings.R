# Installed 1,024/1,025 controls for every width route (#2299).
library(yamaa)
# save() returns the absolute paths it wrote, invisibly, with forward slashes,
# in publication order (D38); each one exists once written.
returns_paths <- function(paths) is.character(paths) && length(paths) > 0L && !anyNA(paths) &&
  all(grepl("^(/|[[:alpha:]]:/)", paths)) && !any(grepl("\\", paths, fixed = TRUE)) &&
  all(file.exists(paths))
# arrow and jsonlite are suggested packages, each used only where it is
# installed. Without jsonlite, which decodes the project route's issues,
# the capacity refusals are compared on the specification route alone;
# without arrow, which writes the Parquet sources, only the CSV sources run.
has_arrow <- requireNamespace("arrow", quietly = TRUE)
has_jsonlite <- requireNamespace("jsonlite", quietly = TRUE)
routes <- if (has_jsonlite) c(FALSE, TRUE) else FALSE
if (!has_jsonlite) message("Skipped the project route of the capacity refusals in column_ceilings.R: jsonlite, a suggested package, is not installed")
if (!has_arrow) message("Skipped the Parquet sources of column_ceilings.R: arrow, a suggested package, is not installed")

# Two routes reach one specification: alone, through yamaa_spec(), or as the
# one specification a project lists, through the project's own check() and
# run(). A listed specification that cannot be prepared has no domain name, so
# the project route uses the whole-project operations, whose issues name the
# listed specification as their entry and source.
project_of <- function(spec) {
  directory <- dirname(spec)
  writeLines("schema_version: '1.0'", file.path(directory, "environment.yaml"))
  project <- file.path(directory, "project.yaml")
  writeLines(c("schema_version: '1.0'", "environments: {r: environment.yaml}",
               paste0("adam: {specs: [", basename(spec), "]}")), project)
  yamaa_project(project)
}
operations <- function(project) {
  if (project) list(function(spec) project_of(spec)$check(), function(spec) project_of(spec)$run())
  else list(function(spec) yamaa_spec(spec)$check(), function(spec) yamaa_spec(spec)$run())
}
# The issues without the project route's identification: `entry` names the
# listed specification, and so does `source` unless the finding already names
# its own source operand. The specification alone adds neither.
route_issues <- function(issues, spec, project) {
  if (!has_jsonlite) {
    stopifnot(!project)
    return(issues)
  }
  issues$context <- vapply(issues$context, function(text) {
    context <- jsonlite::fromJSON(text, simplifyVector = FALSE)
    stopifnot(identical(!is.null(context$entry), project))
    if (!project) return(text)
    stopifnot(endsWith(context$entry, paste0("/", basename(spec))), !is.null(context$source))
    if (identical(context$source, context$entry)) context$source <- NULL
    context$entry <- NULL
    as.character(jsonlite::toJSON(context, auto_unbox = TRUE, digits = NA))
  }, character(1L), USE.NAMES = FALSE)
  issues
}

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

assert_build <- function(spec, width = 1L, project = FALSE) {
  if (project) {
    stopifnot(nrow(project_of(spec)$check()$issues) == 0L)
    runs <- project_of(spec)$run()
    stopifnot(identical(runs$names, "TEST"), nrow(runs$issues) == 0L)
    built <- runs[["TEST"]]
  } else {
    stopifnot(nrow(yamaa_spec(spec)$check()$issues) == 0L)
    built <- yamaa_spec(spec)$run()
  }
  stopifnot(nrow(built$issues) == 0L, nrow(built$output) == 1L,
            ncol(built$output) == width, identical(built$output$ID, "S-001"))
  if (width > 1L) stopifnot(all(built$output[1L, -1L] == "x"))
}

assert_refusal <- function(result, compiler = FALSE) {
  issues <- result$issues
  stopifnot(nrow(issues) > 0L, any(issues$condition == "engine_rejected"))
  if (compiler) stopifnot(any(grepl('"code":"compiler_limit"', issues$context, fixed = TRUE)))
  # A refused run has no output; its save signals the run's own issues.
  if (inherits(result, "yamaa_run")) {
    failure <- tryCatch(result$save(), error = identity)
    stopifnot(is.null(result$output), inherits(failure, "yamaa_error"),
              identical(conditionMessage(failure), "cannot save a failed run; see issues"),
              identical(failure$issues, issues))
  }
  if (inherits(result, "yamaa_project_run")) {
    failure <- tryCatch(result$save(), error = identity)
    stopifnot(inherits(failure, "yamaa_error"), identical(failure$issues, issues))
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
  # A null parent names no parent: the entry stays standalone.
  writeLines(c(document, "parent: null"), file.path(root, "explicit.yaml"))
  writeLines(c("schema_version: '1.0'", "parent: parent.yaml"),
             file.path(root, "child.yaml"))
  writeLines(c("schema_version: '1.0'", "parent: child.yaml"),
             file.path(root, "grandchild.yaml"))
  rows <- list(
    c("S-001", ifelse(seq_len(width - 1L) %% 3L == 1L, "P", "x")),
    c("S-002", rep("y", width - 1L)), c("S-003", rep("Q", width - 1L)))
  expected <- charToRaw(paste0(paste(c(paste(names, collapse = ","),
    vapply(rows, paste, character(1L), collapse = ",")), collapse = "\n"), "\n"))
  for (file in c("parent.yaml", "explicit.yaml", "child.yaml", "grandchild.yaml")) {
    spec <- file.path(root, file)
    stopifnot(nrow(yamaa_spec(spec)$check()$issues) == 0L)
    built <- yamaa_spec(spec)$run()
    stopifnot(nrow(built$issues) == 0L, identical(names(built$output), names),
              nrow(built$output) == 3L, ncol(built$output) == width)
    for (row in seq_along(rows)) {
      stopifnot(identical(unlist(built$output[row, ], use.names = FALSE), rows[[row]]))
    }
    stopifnot(returns_paths(built$save()))
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
  # Each layer names one parent, so only a chain of 8,193 distinct layers
  # reaches the occurrence bound; without the bound it would reach the base.
  base <- write_case(path("long-parent-chain"))
  for (index in 0:8192) {
    writeLines(c("schema_version: '1.0'", sprintf("parent: layer-%d.yaml", index + 1L)),
               file.path(dirname(base), sprintf("layer-%d.yaml", index)))
  }
  file.copy(base, file.path(dirname(base), "layer-8193.yaml"))
  for (operation in operations(FALSE)) {
    result <- operation(file.path(dirname(base), "layer-0.yaml"))
    assert_refusal(result)
    stopifnot(nrow(result$issues) == 1L, identical(result$issues$context,
      '{"actual":8193,"code":"resource_limit","limit":8192,"phase":"inheritance","resource":"parent_occurrences","stage":"prepare"}'))
  }

  # A fixed refusal does not certify that later semantic checks passed. Once
  # capacity is repaired, both operations and routes expose the remaining error.
  case_root <- path("incomplete-checks")
  # Every specification is pruned after composition (LAYER-11 and LAYER-12):
  # keep all 1,025 columns live so the columns capacity is still exercised
  # instead of being pruned away.
  spec <- write_case(case_root, columns = 1025L)
  lines <- readLines(spec)
  lines[grepl("name: C0001,", lines, fixed = TRUE)] <-
    "  - {name: C0001, type: int, derivation: {literal: 1}}"
  lines[grepl("name: C0002,", lines, fixed = TRUE)] <-
    "  - {name: C0002, type: str, derivation: {str_case: {source: C0001, to: upper}}}"
  for (project in routes) {
    writeLines(lines, spec)
    for (operation in operations(project)) {
      refused <- operation(spec)
      assert_refusal(refused, TRUE)
      issue <- route_issues(refused$issues, spec, project)
      stopifnot(nrow(issue) == 1L, is.na(issue$rule),
        !grepl("incomplete_checks_from", issue$context, fixed = TRUE),
        grepl('"actual":1025', issue$context, fixed = TRUE),
        grepl('"limit":1024', issue$context, fixed = TRUE))
    }
    repaired <- lines[!grepl("name: C1024,", lines, fixed = TRUE)]
    # Dropping the C1024 declaration must also drop it from the live output
    # list, otherwise the repaired specification would fail on the dangling
    # output reference instead of the remaining C0002 type error.
    repaired[startsWith(repaired, "output:")] <- gsub(", C1024\\]", "]", repaired[startsWith(repaired, "output:")])
    writeLines(repaired, spec)
    for (operation in operations(project)) {
      issue <- route_issues(operation(spec)$issues, spec, project)
      stopifnot(nrow(issue) == 1L, identical(issue$condition, "incompatible_input_type"),
        identical(issue$rule, "TEXT-1"),
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
      # Keep the counted resource live through pruning: columns needs its
      # full output, inputs needs a row-phase default that reads every input.
      selected = if (resource == "columns") NULL else if (resource == "projected_columns") rep("ID", actual) else "ID")
    if (resource == "inputs") {
      lines <- readLines(spec)
      lines[startsWith(lines, "input:")] <- sprintf("input: {%s}",
        paste(sprintf("S%s: input.csv", seq_len(actual)), collapse = ", "))
      sources <- paste(sprintf("S%s.ID", seq_len(actual)), collapse = ", ")
      lines[grepl("name: ID,", lines, fixed = TRUE)] <- sprintf(
        "  - {name: ID, type: str, label: ID, derivation: {first_available: {sources: [%s]}}}", sources)
      writeLines(lines, spec)
    }
    spec_path <- switch(resource, inputs = "input", source_fields = "input.SRC.types",
                        projected_columns = "output.columns", "columns")
    expected <- sprintf('{"actual":%s,"code":"compiler_limit","limit":%s,"resource":"%s","stage":"prepare"}',
                        actual, limit, resource)
    for (project in routes) {
      for (operation in operations(project)) {
        issue <- route_issues(operation(spec)$issues, spec, project)
        stopifnot(nrow(issue) == 1L, identical(issue$phase, "engine"),
          identical(issue$condition, "engine_rejected"), is.na(issue$rule),
          identical(issue$spec_paths, list(spec_path)), identical(issue$context, expected))
      }
    }
  }

  # Iterative inheritance is bounded by parent occurrences, not a separate depth quota.
  case_root <- path("preparation-depth")
  spec <- write_case(case_root)
  file.copy(spec, file.path(case_root, "base.yaml"))
  for (index in 0:64) {
    parent <- if (index < 64L) sprintf("layer-%s.yaml", index + 1L) else "base.yaml"
    writeLines(c("schema_version: '1.0'", sprintf("parent: %s", parent)),
               file.path(case_root, sprintf("layer-%s.yaml", index)))
  }
  writeLines(c("schema_version: '1.0'", "parent: layer-0.yaml"), spec)
  for (project in c(FALSE, TRUE)) assert_build(spec, project = project)

  # A wide scalar within the file boundary reaches its language finding with exact digits.
  case_root <- path("preparation-numeric-digits")
  spec <- write_case(case_root)
  digits <- paste(rep("9", 4097L), collapse = "")
  writeLines(paste0("schema_version: ", digits), spec)
  expected <- sprintf('{"actual":%s,"expected":"1.0"}', digits)
  for (project in c(FALSE, TRUE)) {
    for (operation in operations(project)) {
      issue <- operation(spec)$issues
      # Keep the wide number as exact text: drop the project identification
      # literally rather than through a JSON number codec.
      identified <- ',"entry":"[^"]*","expected":"1.0","source":"[^"]*"}$'
      stopifnot(identical(grepl(identified, issue$context), project))
      issue$context <- sub(identified, ',"expected":"1.0"}', issue$context)
      stopifnot(nrow(issue) == 1L, identical(issue$phase, "validation"),
        identical(issue$condition, "schema_version_mismatch"),
        identical(issue$rule, "SCHEMA-2"),
        identical(issue$spec_paths, list("schema_version")), identical(issue$context, expected))
    }
  }

  # The declared-column ceiling counts live columns after pruning: a narrow
  # output with one live column builds, so exercise the ceiling with a live
  # 1,025-column output.
  assert_build(write_case(path("admit-columns"), 1024L, selected = "ID"))
  spec <- write_case(path("refuse-columns"), 1025L)
  for (operation in operations(FALSE)) assert_refusal(operation(spec), TRUE)

  assert_build(write_case(path("admit-output"), 1024L), 1024L)
  names <- c("ID", sprintf("C%04d", seq_len(1023L)))
  # Keep declarations in range: projection count refuses before the repeated
  # output name is validated, independently of the declared-column cap.
  spec <- write_case(path("refuse-output"), 1024L, selected = c(names, "ID"))
  for (operation in operations(FALSE)) assert_refusal(operation(spec), TRUE)

  assert_build(write_case(path("admit-types"), fields = 1024L, typed = TRUE))
  spec <- write_case(path("refuse-types"), fields = 1025L, typed = TRUE)
  for (operation in operations(FALSE)) assert_refusal(operation(spec), TRUE)

  for (format in if (has_arrow) c("csv", "parquet") else "csv") {
    assert_build(write_case(path(paste0("admit-", format)), fields = 1024L, source_format = format))
    spec <- write_case(path(paste0("refuse-", format)), fields = 1025L, source_format = format)
    assert_refusal(yamaa_spec(spec)$run())
  }

  linked_consumer <- function(producer) {
    consumer <- file.path(dirname(producer), "consumer.yaml")
    writeLines(c(
      "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
      "input: {SRC: {path: out.csv, schema: spec.yaml}}", "columns:",
      "  - {name: ID, type: str, label: ID, derivation: SRC.ID}",
      "output: {path: consumer.csv, columns: [ID]}"
    ), consumer)
    consumer
  }
  for (width in c(1024L, 1025L)) {
    producer <- write_case(path(paste0("producer-", width)), columns = width)
    consumer <- linked_consumer(producer)
    if (width == 1024L) {
      # Callers build and save the producer before its consumer reads it.
      stopifnot(returns_paths(yamaa_spec(producer)$run()$save()))
      assert_build(consumer)
    } else {
      for (operation in operations(FALSE)) assert_refusal(operation(consumer))
    }
  }

  # Keep the link's metadata in range while its stored artifact crosses the
  # width ceiling; the Rust decoder control pins the contract itself.
  for (width in c(1024L, 1025L)) {
    producer <- write_case(path(paste0("producer-input-", width)), columns = 1024L)
    consumer <- linked_consumer(producer)
    header <- c("ID", sprintf("C%04d", seq_len(width - 1L)))
    writeLines(c(paste(header, collapse = ","),
                 paste(c("S-001", rep("x", width - 1L)), collapse = ",")),
               file.path(dirname(producer), "out.csv"))
    stopifnot(nrow(yamaa_spec(consumer)$check()$issues) == 0L)
    if (width == 1024L) assert_build(consumer) else assert_refusal(yamaa_spec(consumer)$run())
  }
}

run_column_ceilings()
