# Installed 1,024/1,025 controls for every width route (#2299). The boundaries,
# the refusals' issue rows and the wide inherited cases are the
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
admitted <- truth$admitted
refused <- truth$refused
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

assert_refusal <- function(result, label, compiler = FALSE) {
  issues <- result$issues
  if (!(nrow(issues) > 0L && any(issues$condition == "engine_rejected")))
    fail_expectation(paste(label, "is refused as engine_rejected"), issues, "engine_rejected")
  if (compiler && !any(grepl('"code":"compiler_limit"', issues$context, fixed = TRUE)))
    fail_expectation(paste(label, "is refused at a compiler limit"), issues$context, '"code":"compiler_limit"')
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
  # The wide specification; a 0.5 file writes its header as `schema_version`
  # and a text constant as a `literal` (D18).
  case_column <- function(index, legacy) {
    text <- function(value) if (legacy) sprintf("{literal: %s}", value) else sprintf("\"'%s'\"", value)
    c(sprintf("  - name: %s", sprintf(layered$column_name, index)), "    type: str", "    derivation:", "      case:",
      sprintf("        - when: \"SRC.A = 'x' AND SRC.B = '%d'\"", index %% 3L),
      paste0("          then: ", text("P")), "        - when: \"SRC.A IS NULL\"",
      paste0("          then: ", text("Q")), "        - otherwise: SRC.A")
  }
  document <- function(legacy) c(
    if (legacy) "schema_version: '1.0'" else c('yamaa: "1.0"', "type: spec"),
    "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    unlist(lapply(seq_len(width - 1L), case_column, legacy = legacy), use.names = FALSE),
    sprintf("output: {path: out.csv, columns: [%s]}", paste(names, collapse = ", ")))
  writeLines(document(FALSE), file.path(root, "parent.yaml"))
  # 0.5 only, until #3447 deletes the 0.5 path: a null parent names no
  # parent, so the entry stays standalone. A 1.0 file admits YAML null only
  # in value and expression fields (P-NULL-1).
  writeLines(c(document(TRUE), "parent: null"), file.path(root, "explicit.yaml"))
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

# The specification of one boundary at one width, and the width it builds.
boundary <- function(root, kind, width) {
  if (kind == "declared_columns") {
    # The declared-column ceiling counts live columns after pruning: a narrow
    # output with one live column builds, so the ceiling is exercised with a
    # live 1,025-column output.
    return(list(spec = write_case(root, width, selected = if (width == admitted) "ID"), width = 1L))
  }
  if (kind == "output_columns") {
    if (width == admitted) return(list(spec = write_case(root, width), width = width))
    names <- c("ID", sprintf("C%04d", seq_len(admitted - 1L)))
    # Keep declarations in range: projection count refuses before the repeated
    # output name is validated, independently of the declared-column cap.
    return(list(spec = write_case(root, admitted, selected = c(names, "ID")), width = NA_integer_))
  }
  if (kind == "typed_fields") return(list(spec = write_case(root, fields = width, typed = TRUE), width = 1L))
  if (kind %in% c("csv_fields", "parquet_fields")) {
    return(list(spec = write_case(root, fields = width, source_format = sub("_fields$", "", kind)), width = 1L))
  }
  if (kind == "producer_columns") {
    producer <- write_case(root, columns = width)
    consumer <- linked_consumer(producer)
    if (width == admitted) {
      # A link types the consumer before the producer's artifact exists;
      # callers build and save the producer before its consumer reads it.
      expect_identical(nrow(yamaa_spec(consumer)$check()$issues), 0L,
        "a consumer of a 1,024-column producer checks before the producer saves")
      stopifnot("the producer saves before its consumer reads it" =
        returns_paths(yamaa_spec(producer)$run()$save()))
    }
    return(list(spec = consumer, width = 1L))
  }
  if (kind == "stored_columns") {
    # Keep the link's metadata in range while its stored artifact crosses the
    # width ceiling; the Rust decoder control pins the contract itself.
    producer <- write_case(root, columns = admitted)
    consumer <- linked_consumer(producer)
    header <- c("ID", sprintf("C%04d", seq_len(width - 1L)))
    writeLines(c(paste(header, collapse = ","),
                 paste(c("S-001", rep("x", width - 1L)), collapse = ",")),
               file.path(dirname(producer), "out.csv"))
    expect_identical(nrow(yamaa_spec(consumer)$check()$issues), 0L,
      sprintf("a consumer of a %d-column stored artifact checks without issues", width))
    return(list(spec = consumer, width = 1L))
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

  # A fixed refusal does not certify that later semantic checks passed. Once
  # capacity is repaired, both operations and routes expose the remaining error.
  incomplete <- truth$incomplete_checks
  case_root <- path("incomplete-checks")
  # Every specification is pruned after composition (LAYER-11 and LAYER-12):
  # keep all 1,025 columns live so the columns capacity is still exercised
  # instead of being pruned away.
  spec <- write_case(case_root, columns = incomplete$columns)
  lines <- readLines(spec)
  lines[grepl("name: C0001,", lines, fixed = TRUE)] <-
    "  - {name: C0001, type: int, derivation: 1}"
  lines[grepl("name: C0002,", lines, fixed = TRUE)] <-
    "  - {name: C0002, type: str, derivation: {str_case: {source: C0001, to: upper}}}"
  for (project in c(FALSE, TRUE)) {
    writeLines(lines, spec)
    route <- route_name(project)
    for (operation in operations(project)) {
      refused_result <- operation(spec)
      assert_refusal(refused_result, paste("1,025 columns on the", route), TRUE)
      issue <- route_issues(refused_result$issues, spec, project)
      context <- if (nrow(issue) == 1L) jsonlite::fromJSON(issue$context, simplifyVector = FALSE)
      expected <- incomplete$refusal
      if (!(nrow(issue) == 1L && !grepl("incomplete_checks_from", issue$context, fixed = TRUE) &&
        identical(list(rule = if (is.na(issue$rule)) NULL else issue$rule,
          actual = context$actual, limit = context$limit), expected)))
        fail_expectation(paste("1,025 columns on the", route,
          "give one ruleless capacity issue of 1,025 against 1,024 without incomplete_checks_from"),
          issue, expected)
    }
    repaired <- lines[!grepl("name: C1024,", lines, fixed = TRUE)]
    # Dropping the C1024 declaration must also drop it from the live output
    # list, otherwise the repaired specification would fail on the dangling
    # output reference instead of the remaining C0002 type error.
    repaired[startsWith(repaired, "output:")] <- gsub(", C1024\\]", "]", repaired[startsWith(repaired, "output:")])
    writeLines(repaired, spec)
    for (operation in operations(project)) {
      issue <- route_issues(operation(spec)$issues, spec, project)
      expect_identical(as.list(issue[c("condition", "rule")]), incomplete$remaining,
        paste("the repaired capacity on the", route, "exposes the one remaining type error"))
      stopifnot("the remaining error names no incomplete checks" =
        !grepl("incomplete_checks_from", issue$context, fixed = TRUE))
    }
    stopifnot("nothing is written" = !file.exists(file.path(case_root, "out.csv")))
  }

  # Compiler capacity facts are identical through both operations and routes.
  for (case in truth$compiler_limits) {
    resource <- case$resource
    actual <- case$actual
    case_root <- path(paste0("details-", resource))
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
    for (project in c(FALSE, TRUE)) {
      for (operation in names(operations(project))) {
        issue <- route_issues(operations(project)[[operation]](spec)$issues, spec, project)
        expect_identical(issue_fields(issue), truth_fields(list(case$issue)),
          sprintf("the %s of the %s compiler limit on the %s", operation, resource, route_name(project)))
      }
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

  # A wide scalar within the file boundary reaches its language finding with exact digits.
  digits <- strrep("9", truth$wide_integers$digits)
  for (project in c(FALSE, TRUE)) {
    route <- route_name(project)
    case_root <- path(paste0("preparation-numeric-digits-", if (project) "project" else "spec"))
    spec <- write_case(case_root)
    for (case in truth$wide_integers$cases) {
      writeBin(fixture_bytes(gsub("{digits}", digits, case$yaml, fixed = TRUE)), spec)
      expected <- case$issue
      expected$context <- gsub("{digits}", digits, expected$context, fixed = TRUE)
      for (operation in operations(project)) {
        issue <- operation(spec)$issues
        # Keep the wide number as exact text: drop the project identification
        # literally rather than through a JSON number codec.
        identified <- ',"entry":"[^"]*","expected":"1.0","source":"[^"]*"}$'
        expect_identical(grepl(identified, issue$context), project,
          paste(case$name, "names the listed entry in its context exactly on the project route:", route))
        issue$context <- sub(identified, ',"expected":"1.0"}', issue$context)
        expect_identical(issue_fields(issue), truth_fields(list(expected)),
          paste(case$name, "keeps its exact digits on the", route))
      }
    }
    stopifnot_case(route, "nothing is written" = !file.exists(file.path(case_root, "out.csv")))
  }

  # Each width ceiling admits 1,024 and refuses 1,025.
  for (case in truth$boundaries) {
    if (case$kind == "parquet_fields" && !has_arrow) next
    admit <- boundary(path(paste0("admit-", case$kind)), case$kind, admitted)
    assert_build(admit$spec, admit$width)
    refuse <- boundary(path(paste0("refuse-", case$kind)), case$kind, refused)
    for (operation in fixture_strings(case$operations)) {
      assert_refusal(operations(FALSE)[[operation]](refuse$spec),
        sprintf("the %s of %d %s", operation, refused, case$name), isTRUE(case$compiler_limit))
    }
  }
}

run_column_ceilings()
