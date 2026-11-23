# Installed controls for every width route (#2299, #4773). A table has no
# fixed column limit (LIMIT-2), so each route builds at 1,025 columns, past the
# former 1,024-column limit. The routes and their output widths, the
# parent-occurrence issue row and the wide inherited cases are the
# `column-ceilings` family of the shared host truth, which the Python
# `installed_column_ceilings` suite reads too.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it and decodes the project
# route's issues, is a suggested package: where it is not installed, the file
# is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped column_ceilings.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "column-ceilings")
route_width <- truth$width
# arrow, a suggested package, writes the Parquet sources; without it only the
# CSV sources run.
has_arrow <- requireNamespace("arrow", quietly = TRUE)
if (!has_arrow) message("Skipped the Parquet sources of column_ceilings.R: arrow, a suggested package, is not installed")

# Two routes reach one specification: alone, through yamaa_spec(), or as the
# one specification a project lists, through the project's own check() and
# run(). A listed specification that cannot be prepared has no domain name, so
# the project route uses the whole-project operations, whose issues name the
# listed specification as their entry and source.
project_of <- function(spec) {
  # The project calls no project function, so it names no environment (D110).
  project <- file.path(dirname(spec), "project.yaml")
  writeLines(c('yamaa: "1.0"', "type: project",
               paste0("adam: {specs: [", basename(spec), "]}")), project)
  yamaa_project(project)
}
operations <- function(project) {
  if (project) list(check = function(spec) project_of(spec)$check(), run = function(spec) project_of(spec)$run())
  else list(check = function(spec) yamaa_spec(spec)$check(), run = function(spec) yamaa_spec(spec)$run())
}
route_name <- function(project) if (project) "project route" else "specification route"
# The issues without the project route's identification: `entry` names the
# listed specification, and so does `source` unless the finding already names
# its own source operand. The specification alone adds neither.
route_issues <- function(issues, spec, project) {
  issues$context <- vapply(issues$context, function(text) {
    context <- jsonlite::fromJSON(text, simplifyVector = FALSE)
    expect_identical(!is.null(context$entry), project,
      paste("the context names an entry exactly on the project route:", text))
    if (!project) return(text)
    stopifnot("the entry is the listed specification, and a source is named" =
      endsWith(context$entry, paste0("/", basename(spec))) && !is.null(context$source))
    if (identical(context$source, context$entry)) context$source <- NULL
    context$entry <- NULL
    as.character(jsonlite::toJSON(context, auto_unbox = TRUE, digits = NA))
  }, character(1L), USE.NAMES = FALSE)
  issues
}
# The authored fields of issue rows, as an issues table holds them.
issue_fields <- function(issues) as.list(issues[c("phase", "condition", "rule", "spec_paths", "context")])
truth_fields <- function(expected) list(
  phase = vapply(expected, function(issue) issue$phase, ""),
  condition = vapply(expected, function(issue) issue$condition, ""),
  rule = vapply(expected, function(issue) if (is.null(issue$rule)) NA_character_ else issue$rule, ""),
  spec_paths = lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
  context = vapply(expected, function(issue) issue$context, ""))

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
    sprintf("  - {name: %s, type: str, label: Column, derivation: \"'x'\"}", names[-1L])
  )
  if (is.null(selected)) selected <- names
  # A section lists only specifications of its model (SUB-3), so each case
  # declares one for the project route.
  spec <- c(
    'yamaa: "1.0"', "type: spec", "model: ADaM", "domain: TEST", "keys: [ID]",
    sprintf("input: {SRC: %s}", source), "columns:", column_lines,
    sprintf("output: {path: out.csv, columns: [%s]}", paste(selected, collapse = ", "))
  )
  path <- file.path(root, "spec.yaml")
  writeLines(spec, path)
  path
}

# Checks and builds one specification, and returns what saves it: the run, or
# on the project route the project's run.
assert_build <- function(spec, width = 1L, project = FALSE) {
  label <- sprintf("%s (width %d, %s)", spec, width, route_name(project))
  if (project) {
    expect_identical(nrow(project_of(spec)$check()$issues), 0L, paste(label, "checks without issues"))
    runs <- project_of(spec)$run()
    expect_identical(runs$names, "TEST", paste(label, "builds TEST"))
    expect_identical(nrow(runs$issues), 0L, paste(label, "runs without issues"))
    built <- runs[["TEST"]]
  } else {
    expect_identical(nrow(yamaa_spec(spec)$check()$issues), 0L, paste(label, "checks without issues"))
    built <- runs <- yamaa_spec(spec)$run()
  }
  expect_identical(nrow(built$issues), 0L, paste(label, "builds without issues"))
  expect_identical(dim(built$output), c(1L, width), paste(label, "builds one row of every column"))
  expect_identical(built$output$ID, "S-001", paste(label, "keeps the source ID"))
  if (width > 1L) stopifnot("every other cell is the literal x" = all(built$output[1L, -1L] == "x"))
  invisible(runs)
}

assert_refusal <- function(result, label) {
  issues <- result$issues
  if (!(nrow(issues) > 0L && any(issues$condition == "engine_rejected")))
    fail_expectation(paste(label, "is refused as engine_rejected"), issues, "engine_rejected")
  # A refused run has no output; its save signals the run's own issues.
  if (inherits(result, "yamaa_run")) {
    failure <- tryCatch(result$save(), error = identity)
    stopifnot("a refused run has no output" = is.null(result$output),
      "its save signals yamaa_error with D39's text and the run's issues" =
        inherits(failure, "yamaa_error") && save_refusal_text(failure, issues) &&
        identical(failure$issues, issues))
  }
  if (inherits(result, "yamaa_project_run")) {
    failure <- tryCatch(result$save(), error = identity)
    stopifnot("a refused project run's save signals yamaa_error with its issues" =
      inherits(failure, "yamaa_error") && identical(failure$issues, issues))
  }
}

assert_layered_cases <- function(root, width, layered) {
  dir.create(root, recursive = TRUE)
  writeBin(fixture_bytes(layered$input_csv), file.path(root, "input.csv"))
  names <- c("ID", sprintf(layered$column_name, seq_len(width - 1L)))
  # The wide specification, whose text constants are quoted (D18).
  case_column <- function(index) {
    text <- function(value) sprintf("\"'%s'\"", value)
    c(sprintf("  - name: %s", sprintf(layered$column_name, index)), "    type: str", "    derivation:", "      case:",
      sprintf("        - when: \"SRC.A = 'x' AND SRC.B = '%d'\"", index %% 3L),
      paste0("          then: ", text("P")), "        - when: \"SRC.A IS NULL\"",
      paste0("          then: ", text("Q")), "        - otherwise: SRC.A")
  }
  document <- c(
    'yamaa: "1.0"', "type: spec",
    "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    unlist(lapply(seq_len(width - 1L), case_column), use.names = FALSE),
    sprintf("output: {path: out.csv, columns: [%s]}", paste(names, collapse = ", ")))
  writeLines(document, file.path(root, "parent.yaml"))
  writeLines(c('yamaa: "1.0"', "type: spec", "parent: parent.yaml"),
             file.path(root, "child.yaml"))
  writeLines(c('yamaa: "1.0"', "type: spec", "parent: child.yaml"),
             file.path(root, "grandchild.yaml"))
  rows <- lapply(layered$rows, function(row) {
    cells <- fixture_strings(row$cells_by_index_mod_3)
    c(row$ID, cells[seq_len(width - 1L) %% 3L + 1L])
  })
  expected <- charToRaw(paste0(paste(c(paste(names, collapse = ","),
    vapply(rows, paste, character(1L), collapse = ",")), collapse = "\n"), "\n"))
  for (file in fixture_strings(layered$files)) {
    spec <- file.path(root, file)
    label <- sprintf("%s of width %d", file, width)
    expect_identical(nrow(yamaa_spec(spec)$check()$issues), 0L, paste(label, "checks without issues"))
    built <- yamaa_spec(spec)$run()
    expect_identical(nrow(built$issues), 0L, paste(label, "builds without issues"))
    expect_identical(names(built$output), names, paste(label, "builds every column in order"))
    expect_identical(dim(built$output), c(length(rows), width), paste(label, "builds every row"))
    for (row in seq_along(rows)) {
      expect_identical(unlist(built$output[row, ], use.names = FALSE), rows[[row]],
        sprintf("%s: row %d", label, row))
    }
    stopifnot("save returns the paths it wrote" = returns_paths(built$save()))
    expect_identical(rawfile(file.path(root, "out.csv")), expected, paste(label, "saves the exact CSV"))
  }
}

linked_consumer <- function(producer) {
  consumer <- file.path(dirname(producer), "consumer.yaml")
  writeLines(c(
    'yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
    "input: {SRC: {path: out.csv, schema: spec.yaml}}", "columns:",
    "  - {name: ID, type: str, label: ID, derivation: SRC.ID}",
    "output: {path: consumer.csv, columns: [ID]}"
  ), consumer)
  consumer
}

# The specification of one width route at the family's width.
boundary <- function(root, kind) {
  if (kind == "declared_columns") {
    # 1,025 declarations behind a one-column output, which prunes all but ID;
    # the output_columns route keeps all 1,025 live.
    return(write_case(root, route_width, selected = "ID"))
  }
  if (kind == "output_columns") return(write_case(root, route_width))
  if (kind == "typed_fields") return(write_case(root, fields = route_width, typed = TRUE))
  if (kind %in% c("csv_fields", "parquet_fields")) {
    return(write_case(root, fields = route_width, source_format = sub("_fields$", "", kind)))
  }
  if (kind == "producer_columns") {
    producer <- write_case(root, columns = route_width)
    consumer <- linked_consumer(producer)
    # A link types the consumer before the producer's artifact exists;
    # callers build and save the producer before its consumer reads it.
    expect_identical(nrow(yamaa_spec(consumer)$check()$issues), 0L,
      sprintf("a consumer of a %d-column producer checks before the producer saves", route_width))
    stopifnot("the producer saves before its consumer reads it" =
      returns_paths(yamaa_spec(producer)$run()$save()))
    return(consumer)
  }
  if (kind == "stored_columns") {
    # The stored artifact is written by hand rather than saved by its
    # producer, so the consumer decodes a file no build wrote.
    producer <- write_case(root, columns = route_width)
    consumer <- linked_consumer(producer)
    header <- c("ID", sprintf("C%04d", seq_len(route_width - 1L)))
    writeLines(c(paste(header, collapse = ","),
                 paste(c("S-001", rep("x", route_width - 1L)), collapse = ",")),
               file.path(dirname(producer), "out.csv"))
    expect_identical(nrow(yamaa_spec(consumer)$check()$issues), 0L,
      sprintf("a consumer of a %d-column stored artifact checks without issues", route_width))
    return(consumer)
  }
  stop("unknown boundary kind: ", kind)
}

run_column_ceilings <- function() {
  root <- tempfile("yamaa-column-ceilings-")
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)

  for (width in fixture_strings(truth$layered$widths)) {
    assert_layered_cases(path(paste0("layered-", width)), as.integer(width), truth$layered)
  }

  # Each layer names one parent, so only a chain of 8,193 distinct layers
  # reaches the occurrence bound; without the bound it would reach the base.
  occurrences <- truth$parent_occurrences
  bound <- occurrences$layers_between_entry_and_base
  for (project in c(FALSE, TRUE)) {
    base <- write_case(path(paste0("long-parent-chain-", if (project) "project" else "spec")))
    for (index in 0:bound) {
      writeLines(c('yamaa: "1.0"', "type: spec", sprintf("parent: layer-%d.yaml", index + 1L)),
                 file.path(dirname(base), sprintf("layer-%d.yaml", index)))
    }
    file.copy(base, file.path(dirname(base), sprintf("layer-%d.yaml", bound + 1L)))
    entry <- file.path(dirname(base), "layer-0.yaml")
    for (operation in names(operations(project))) {
      label <- sprintf("the %s of a chain of %d parents on the %s", operation, bound + 1L, route_name(project))
      result <- operations(project)[[operation]](entry)
      assert_refusal(result, label)
      expect_identical(issue_fields(route_issues(result$issues, entry, project)),
        truth_fields(list(occurrences$issue)), paste(label, "reaches the parent-occurrence bound, in one issue"))
    }
  }

  # Iterative inheritance is bounded by parent occurrences, not a separate depth quota.
  depth <- truth$inheritance_depth
  for (project in c(FALSE, TRUE)) {
    case_root <- path(paste0("preparation-depth-", if (project) "project" else "spec"))
    spec <- write_case(case_root)
    file.copy(spec, file.path(case_root, "base.yaml"))
    layers <- depth$layers_between_entry_and_base
    for (index in seq_len(layers) - 1L) {
      parent <- if (index < layers - 1L) sprintf("layer-%s.yaml", index + 1L) else "base.yaml"
      writeLines(c('yamaa: "1.0"', "type: spec", sprintf("parent: %s", parent)),
                 file.path(case_root, sprintf("layer-%s.yaml", index)))
    }
    writeLines(c('yamaa: "1.0"', "type: spec", "parent: layer-0.yaml"), spec)
    built <- assert_build(spec, project = project)
    stopifnot_case(paste(layers, "layers on the", route_name(project)), "the build saves" = returns_paths(built$save()))
    expect_identical(rawfile(file.path(case_root, "out.csv")), fixture_bytes(depth$output_csv),
      paste(layers, "layers on the", route_name(project), "save the base's output"))
  }

  # Each width route builds 1,025 columns, past the former 1,024-column limit.
  for (case in truth$boundaries) {
    if (case$kind == "parquet_fields" && !has_arrow) next
    spec <- boundary(path(paste0("width-", case$kind)), case$kind)
    assert_build(spec, as.integer(case$output_width))
  }
}

run_column_ceilings()
