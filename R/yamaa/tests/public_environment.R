# Public installed projects and their environments against independent truth.
# A project file lists its function files and holds each host's lock; each
# host reads only its own lock, and project functions, codelists and locks
# reach a specification only through yamaa_project(path)$spec(name) or $run().
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped public_environment.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-environment")
namespace <- asNamespace("yamaa")
spec_run_native <- get("wrap__spec_run", namespace)
for (capabilities in list(list(), list(NULL, function(...) NULL, function(...) NULL))) {
  reply <- .Call(spec_run_native, charToRaw("absent-spec.yaml"), NULL,
    list(charToRaw(as.character(getRversion())), capabilities), NULL, FALSE, NULL, NULL, NULL)
  label <- sprintf("%d malformed capabilities", length(capabilities))
  stopifnot_case(label, "the reply has no value" = is.null(reply$value),
    "the reply has no issues" = nrow(reply$error$issues) == 0L)
  expect_identical(reply$error$message, if (length(capabilities) == 0L)
    "invalid project host capabilities" else "invalid lock capability", paste(label, "name what is invalid"))
}
root <- normalizePath("project-base", winslash = "/", mustWork = TRUE)
readtext <- function(path) readLines(path, warn = FALSE)
observations <- function(run) yamaa:::.run_observations(run)
here <- function(name) file.path(normalizePath(getwd(), winslash = "/"), name)
# One issue row as a list of plain R values, for exact comparison. Each is an
# error with a message, located in a file the case wrote (P-ISSUE-1); the
# cases author the other columns, and issue_locations.R pins the locations.
row_of <- function(frame, index = 1L) {
  stopifnot_case(sprintf("issue %d", index), "the issue is an error" = identical(frame$severity[[index]], "error"),
    "the issue has a message" = nzchar(frame$message[[index]]),
    "the issue is located in a file" = nzchar(frame$file[[index]]),
    "the issue's line and column count from 1" = frame$line[[index]] >= 1 && frame$column[[index]] >= 1)
  authored <- setdiff(names(frame), c("severity", "file", "line", "column", "message"))
  lapply(frame[authored], function(column) column[[index]])
}
issue <- function(condition, rule, spec_paths, context, phase = "validation")
  list(phase = phase, condition = condition, rule = rule, spec_paths = spec_paths,
    context = context, source_span = NULL, operand_route = NULL)
same_issues <- function(frame, expected, label = "the issue rows") {
  actual <- lapply(seq_len(nrow(frame)), function(index) row_of(frame, index))
  expect_identical(actual, expected, paste(label, "are the exact authored tuples"))
}
refuse <- function(...) stop("unexpected activation")
refuse_activation <- function(code) with_capabilities(refuse, refuse, code)
# Project problems: issues of $check() and of a run that built nothing.
refused_project <- function(project, expected, label = "a refused project") {
  same_issues(project$check()$issues, expected, paste(label, "check:"))
  runs <- project$run()
  expect_identical(runs$names, character(), paste(label, "builds nothing"))
  same_issues(runs$issues, expected, paste(label, "run:"))
  error <- tryCatch(runs$save(), yamaa_error = identity)
  stopifnot_case(label, "its run's save signals yamaa_error" = inherits(error, "yamaa_error"))
  same_issues(error$issues, expected, paste(label, "save:"))
  error <- tryCatch(project$spec("TEST"), yamaa_error = identity)
  stopifnot_case(label, "selecting its specification signals yamaa_error" = inherits(error, "yamaa_error"))
  same_issues(error$issues, expected, paste(label, "selection:"))
}
function_spec <- c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: input.csv, types: {VALUE: float}}}", "columns:",
  "  - {name: ID, type: str, derivation: SRC.ID}",
  "  - {name: VALUE, type: float, derivation: {function: {name: project_value, args: {x: SRC.VALUE}}}}",
  "output: {path: output.csv, columns: [ID, VALUE]}")
# The function file defining project_value, which function_spec calls.
function_file <- c('yamaa: "1.0"', "type: function", "name: project_value",
  "description: Return the supplied numeric value.", "params: [{name: x, type: float}]", "returns: float",
  "implementations: {python: yamaa_benchmarks.project_value, r: 'yamaabenchmarks::project_value'}", "tests:",
  "  - {id: ordinary, covers: [normal, numeric-comparison], args: {x: 1.5}, result: 1.5}",
  "  - {id: zero-boundary, covers: [boundary, numeric-comparison], args: {x: 0.0}, result: 0.0}",
  "  - {id: missing-short-circuits, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}")

qualify_metadata_failure_locations <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("metadata-locations-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(c("yamaa: '1.0'", "type: project", "functions: [missing-function.yaml]",
    "environments: {python: {lock: missing.lock}, r: {lock: missing.lock}}",
    "codelists: [missing-codelists.yaml]", "adam: {specs: [absent-spec.yaml]}"), "project.yaml")
  # A file the project names that does not exist fails at the field that
  # names it, naming the path as written, as any file a run reads does
  # (FUNC-2, FILE-7, FILE-11).
  expected <- Map(function(declaration, path) issue("resource_path_missing", "FILE-7",
    declaration, paste0('{"path":"', path, '"}')),
    c("environments.r.lock", "functions[0]", "codelists[0]"),
    c("missing.lock", "missing-function.yaml", "missing-codelists.yaml"))
  refuse_activation(refused_project(yamaa_project("project.yaml"), unname(expected),
    "a project of missing metadata files"))
  expect_identical(sort(list.files()), "project.yaml", "a refused project writes nothing")
  cat("public R project metadata failures name each declared input before any entry or activation\n")
}
qualify_metadata_failure_locations()

qualify_unavailable_function_file <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("function-file-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(c("yamaa: '1.0'", "type: project", "functions: [missing-function.yaml]",
    "adam: {specs: [spec.yaml]}"), "project.yaml")
  expected <- list(issue("resource_path_missing", "FILE-7", "functions[0]",
    '{"path":"missing-function.yaml"}'))
  for (content in list(NULL, "[entry must not be read")) {
    unlink("spec.yaml")
    if (!is.null(content)) writeLines(content, "spec.yaml")
    refuse_activation(refused_project(yamaa_project("project.yaml"), expected,
      paste("a missing function file with", if (is.null(content)) "no entry" else "an unreadable entry")))
  }
  expect_identical(sort(list.files()), c("project.yaml", "spec.yaml"), "a refused project writes nothing")
  cat("public R missing function files refuse the project before entry preparation\n")
}
qualify_unavailable_function_file()

qualify_own_lock <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("own-lock-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(function_spec, "spec.yaml")
  writeLines(function_file, "value.yaml")
  # The Python entry names a lock that does not exist; R never reads it
  # (PROJECT-2), and every host reads the function file.
  writeLines(c("yamaa: '1.0'", "type: project", "functions: [value.yaml]",
    "environments: {python: {lock: python/absent.lock}}", "adam: {specs: [spec.yaml]}"), "project.yaml")
  project <- yamaa_project("project.yaml")
  refuse_activation({
    same_issues(project$spec("TEST")$check()$issues, list(), "the specification check's issues")
    same_issues(project$check()$issues, list(), "the project check's issues")
    # The same file alone has no project, so its function call binds nothing
    # a project supplies (CT-13).
    same_issues(yamaa_spec("spec.yaml")$check()$issues, list(issue("project_binding_missing",
      "CT-13", "columns.VALUE.derivation.function.name",
      '{"binding":"function","function":"project_value"}')))
  })
  expect_identical(sort(list.files()), c("project.yaml", "spec.yaml", "value.yaml"),
    "reading the project writes nothing")
  cat("public R reads only its own lock\n")
}
qualify_own_lock()

qualify_static_check_without_study_inputs <- function() {
  work <- tempfile("static-check-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  # The caller's path may be any UTF-8 text; the project file itself is ASCII.
  study <- file.path(work, "\u00e9tude"); dir.create(study)
  for (name in "base-round") {
    stopifnot_case(name, "the case is copied" = file.copy(file.path(root, name), study, recursive = TRUE))
    case <- file.path(study, name)
    unlink(file.path(case, "input"), recursive = TRUE)
    project <- yamaa_project(file.path(case, "project.yaml"))
    checked <- refuse_activation(project$check())
    domain <- sub("^domain: ", "", grep("^domain: ", readtext(file.path(case, "spec.yaml")), value = TRUE))
    spec <- refuse_activation(project$spec(domain)$check())
    stopifnot_case(name, "the project checks without study inputs" = nrow(checked$issues) == 0L,
      "its specification checks without study inputs" = nrow(spec$issues) == 0L,
      "a check writes no CSV" = length(list.files(case, pattern = "[.]csv$")) == 0L)
  }
  cat("public R project check validates original studies without study inputs or publication\n")
}
qualify_static_check_without_study_inputs()

qualify_functions_without_packaging_lock <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("unlocked-functions-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  stopifnot("the case is copied" = file.copy(file.path(root, "base-round"), work, recursive = TRUE))
  setwd(file.path(work, "base-round"))
  # The project names no packaging lock.
  project_text <- readtext("project.yaml")
  writeLines(project_text[!grepl("^environments:|^  r:$|^    lock:", project_text)], "project.yaml")
  unlink("r/renv.lock")
  expected <- list.files("expected", pattern = "[.]csv$", full.names = TRUE)
  project <- yamaa_project("project.yaml")
  stopifnot("the case authors one CSV" = length(expected) == 1L,
    "the unlocked project checks" = nrow(project$check()$issues) == 0L)
  for (attempt in 1:2) {
    result <- project$spec("TEST")$run()
    evidence <- observations(result)
    stopifnot_case(sprintf("unlocked run %d", attempt), "the run has no issues" = nrow(result$issues) == 0L,
      "the run has output" = is.data.frame(result$output),
      "no lock is requested" = grepl('"lock":"not_requested"', evidence, fixed = TRUE),
      "the function tests pass" = grepl('"outcome":"passed"', evidence, fixed = TRUE),
      "the run saves" = returns_paths(result$save()),
      "the saved CSV is the authored one" = identical(rawfile(basename(expected)), rawfile(expected)))
  }
  cat("public R functions without a packaging lock keep conformance and saved bytes\n")
}
qualify_functions_without_packaging_lock()

previous <- getwd()
work <- tempfile("public-environment-"); dir.create(work)
for (name in "base-round") {
  stopifnot_case(name, "the case is copied" = file.copy(file.path(root, name), work, recursive = TRUE))
  case <- file.path(work, name); setwd(case)
  expected <- list.files("expected", pattern = "[.]csv$", full.names = TRUE)
  project <- yamaa_project("project.yaml")
  stopifnot_case(name, "the case authors one CSV" = length(expected) == 1L,
    "the project checks" = nrow(project$check()$issues) == 0L)
  runs <- project$run()
  domain <- runs$names
  stopifnot_case(name, "the project run has no issues" = nrow(runs$issues) == 0L,
    "the project builds one specification" = length(domain) == 1L,
    "the member has output" = is.data.frame(runs[[domain]]$output))
  artifact <- basename(expected)
  for (attempt in 1:2) {
    result <- project$spec(domain)$run()
    before <- observations(result)
    stopifnot_case(sprintf("%s run %d", name, attempt), "the run has no issues" = nrow(result$issues) == 0L,
      "the run's output is the member's" = identical(result$output, runs[[domain]]$output),
      "the lock is verified" = grepl('"lock":"verified"', before, fixed = TRUE),
      "the function tests pass" = grepl('"outcome":"passed"', before, fixed = TRUE),
      "nothing is saved before save" = grepl('"artifacts":[]', before, fixed = TRUE) && !file.exists(artifact),
      "the run saves" = returns_paths(result$save()),
      "the saved CSV is the authored one" = identical(rawfile(artifact), rawfile(expected)))
    unlink(artifact)
  }
  stopifnot_case(name, "the project run saves" = returns_paths(runs$save()),
    "the project's saved CSV is the authored one" = identical(rawfile(artifact), rawfile(expected)))
  cat(name, "public project exact base R CSV and repeated activation passed\n")
}
name <- "negative-function-contract"
stopifnot("the case is copied" = file.copy(file.path(root, name), work, recursive = TRUE))
case <- file.path(work, name); setwd(case); unlink("input", recursive = TRUE)
project <- yamaa_project("project.yaml")
context <- '"argument":"x","cause":"missing_required_argument","function":"project_value"'
expected <- list(issue("invalid_function_argument", "FUNC-5",
  "columns.RESULT.derivation.function.args", paste0("{", context, "}")))
refuse_activation({
  spec <- project$spec("TEST")
  for (result in list(spec$check(), spec$run())) same_issues(result$issues, expected)
  # A project check names the specification each issue belongs to.
  same_issues(project$check()$issues, list(issue("invalid_function_argument", "FUNC-5",
    "columns.RESULT.derivation.function.args",
    paste0('{"argument":"x","cause":"missing_required_argument","entry":"', here("spec.yaml"),
      '","function":"project_value","source":"', here("spec.yaml"), '"}'))))
})
stopifnot("a refused call has no output" = is.null(result$output),
  "a refused call has no report" = is.null(result$report),
  "a refused call's save signals yamaa_error" = inherits(tryCatch(result$save(), error = identity), "yamaa_error"))
# A valid static call still reads no study data and runs no activation.
writeLines(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
  readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
stopifnot("a valid call checks without activation" =
  nrow(refuse_activation(project$spec("TEST")$check())$issues) == 0L)
# Version disagreement precedes binding, tests and even a missing study input.
lock <- "r/renv.lock"
writeLines(sub(paste0('"Version": "', as.character(packageVersion("yamaa")), '"'),
  '"Version": "999999.0"', readtext(lock), fixed = TRUE), lock)
failed <- project$spec("TEST")$run()
expect_identical(failed$issues$condition, "runtime_artifact_mismatch", "a lock of another version")
stopifnot("a version mismatch reads no source" = grepl('"source_reads":[]', observations(failed), fixed = TRUE),
  "a version mismatch binds nothing" = grepl('"bindings":[]', observations(failed), fixed = TRUE))
invisible(file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE))
# Independently failing test results collect completely on every attempted run.
contract <- "functions/project_value.yaml"
text <- readtext(contract)
text <- sub("result: 1.5", "result: 9.5", text, fixed = TRUE)
text <- sub("result: 0.0", "result: 9.0", text, fixed = TRUE)
writeLines(text, contract)
project_text <- readtext("project.yaml")
for (declared_lock in c(TRUE, FALSE)) {
  # Without its lock, the project names no R environment.
  writeLines(if (declared_lock) project_text else
    project_text[!grepl("^  r:$|^    lock: r/", project_text)], "project.yaml")
  for (attempt in 1:2) {
    failed <- project$spec("TEST")$run()
    label <- sprintf("failing function tests, run %d %s a lock", attempt, if (declared_lock) "with" else "without")
    expect_identical(failed$issues$condition, rep("function_conformance_failed", 2L),
      paste(label, "report both failing tests"))
    stopifnot_case(label, "failing tests read no source" = grepl('"source_reads":[]', observations(failed), fixed = TRUE),
      "the lock is verified, or not requested without one" =
        grepl(if (declared_lock) '"lock":"verified"' else '"lock":"not_requested"',
          observations(failed), fixed = TRUE))
  }
}
invisible(file.copy(file.path(root, name, contract), contract, overwrite = TRUE))
invisible(file.copy(file.path(root, name, "project.yaml"), "project.yaml", overwrite = TRUE))
# Captured syntax, rather than the selected host or basename, identifies the lock.
invisible(file.copy(file.path(root, name, "python/uv.lock"), lock, overwrite = TRUE))
same_issues(refuse_activation(project$check())$issues, list(issue("function_file_invalid", "FUNC-2",
  "environments.r.lock", paste0('{"actual_lock_kind":"uv","entry":"', here("project.yaml"),
    '","environment_path":"environments.r.lock","language":"r","reason":"lock_kind_mismatch","source":"',
    here("project.yaml"), '"}'))))
invisible(file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE))
# Project codelists bind and function argument admission collect independent findings.
writeLines(c(readtext("project.yaml"), "codelists:", "  - codelists:", "      - id: IDENTIFIERS",
  "        name: Identifiers", "        items: [{value: '01'}]"), "project.yaml")
invisible(file.copy(file.path(root, name, "input"), case, recursive = TRUE))
text <- readtext(file.path(root, name, "spec.yaml"))
text <- sub("label: Identifier", "label: Identifier\n    submission: {codelist: IDENTIFIERS, origins: [{type: Assigned}]}", text, fixed = TRUE)
text <- sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}", text, fixed = TRUE)
writeLines(text, "spec.yaml")
stopifnot("a project codelist binds in a check" = nrow(project$spec("TEST")$check()$issues) == 0L)
result <- project$spec("TEST")$run()
stopifnot("a project codelist binds in a run" = nrow(result$issues) == 0L,
  "the codelist is verified" = grepl('"verifications":[{', observations(result), fixed = TRUE))
text <- sub("codelist: IDENTIFIERS", "codelist: ABSENT", text, fixed = TRUE)
text <- sub("args: {x: SOURCE.VALUE}", "args: {}", text, fixed = TRUE)
writeLines(text, "spec.yaml"); unlink("input", recursive = TRUE)
checked <- refuse_activation(project$spec("TEST")$check())
expect_identical(sort(unique(checked$issues$rule)), c("CT-1", "FUNC-5"),
  "an absent codelist and missing arguments are both found")
# Independently specified math truth is identical in column, row and nested case
# contexts. These declarations call no project function.
for (context in fixture_strings(host_truth[["quota_modes"]])) {
  target <- file.path(work, paste0("math-", context)); dir.create(target)
  stopifnot_case(context, "the case is copied" = file.copy(file.path(root, name), target, recursive = TRUE))
  setwd(file.path(target, name))
  # The project lists the specification under `adam` (SUB-3).
  text <- c('yamaa: "1.0"', 'type: spec', 'model: ADaM', 'domain: TEST', 'keys: [ID]',
    'input: {SOURCE: {path: input/source.csv, types: {VALUE: float}}}',
    'output: {path: test.csv, columns: [ID, RESULT]}', 'columns:',
    '  - {name: ID, type: str, label: Identifier, derivation: SOURCE.ID}',
    '  - name: RESULT', '    type: float', '    label: Result')
  expr <- 'EXP(0) + LN(1) + POWER(2, 3)'
  if (context == "column") text <- c(text, paste0('    derivation: "', expr, '"'))
  if (context == "row") text <- c(text, 'rows:', '  - id: records', '    dataset: SOURCE',
    paste0('    derivations: {RESULT: "', expr, '"}'))
  if (context == "case") text <- c(text, '    derivation:', '      case:',
    '        - when: "ID = \'01\'"', '          then:', '            case:',
    '              - when: "ID = \'01\'"',
    paste0('                then: "', expr, '"'),
    '              - otherwise: 99.0', '        - otherwise: 98.0')
  writeLines(text, 'spec.yaml')
  result <- yamaa_project('project.yaml')$spec('TEST')$run()
  stopifnot_case(paste("math in a", context, "context"), "the run has no issues" = nrow(result$issues) == 0L,
    "the result is 9" = identical(result$output$RESULT, 9),
    "the run saves" = returns_paths(result$save()),
    "the saved CSV is exact" = identical(rawfile('test.csv'), charToRaw('ID,RESULT\n01,9\n')))
}
precision_dir <- file.path(work, "csv-precision"); dir.create(precision_dir)
setwd(precision_dir)
precision_study <- function(decimals = "2", path = "rounded.csv") {
  writeLines(c("ID,VALUE,INT", "1,0.125,7", "2,-0.125,-7", "3,2.675,0",
    "4,-0.004,9", "5,1.234,1", "6,2.345,2", "7,,3", "8,25,4"), "input.csv")
  precision <- if (is.null(decimals)) "" else paste0(", decimals: ", decimals)
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
    "input: {SRC: {path: input.csv, types: {ID: int, VALUE: float, INT: int}}}",
    paste0("output: {path: ", path, host_truth[["callback_projection"]], precision, "}"),
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    "  - {name: VALUE, type: float, derivation: SRC.VALUE}",
    "  - {name: DOUBLE, type: float, derivation: 'VALUE + VALUE'}",
    "  - {name: EMPTY, type: str, derivation: \"''\"}",
    "  - {name: INT, type: int, derivation: SRC.INT}",
    "verifications: [{assert: {when: 'ID = 5', require: 'DOUBLE = 2.468'}}]"), "spec.yaml")
  yamaa_spec("spec.yaml")
}
spec <- precision_study()
expected <- fixture_bytes(host_truth[["rounded_callback_csv"]])
stopifnot("decimals check without activation" = nrow(refuse_activation(spec$check())$issues) == 0L)
rounded <- refuse_activation(spec$run())
stopifnot("the rounded run has no issues" = nrow(rounded$issues) == 0L)
expect_identical(rounded$output$VALUE, c(0.125, -0.125, 2.675, -0.004, 1.234, 2.345, NA_real_, 25),
  "decimals leave the table's values unrounded")
expect_identical(rounded$output$DOUBLE, c(0.25, -0.25, 5.35, -0.008, 2.468, 4.69, NA_real_, 50),
  "decimals leave computed values unrounded")
stopifnot("nothing is written before save" = !file.exists("rounded.csv"),
  "the rounded run saves" = returns_paths(rounded$save()))
expect_identical(rawfile("rounded.csv"), expected, "the saved CSV is rounded to two decimals")
unlink("rounded.csv")
ordinary <- precision_study(NULL)$run()
stopifnot("the unrounded run has no issues" = nrow(ordinary$issues) == 0L)
expect_identical(ordinary$output, rounded$output, "decimals change only the saved CSV")
spec <- precision_study(host_truth[["csv_precision_maximum"]])
writeLines(c("ID,VALUE,INT", "1,0.125,7"), "input.csv")
maximum <- spec$run()
expected <- fixture_bytes(host_truth[["maximum_precision_csv"]])
stopifnot("the maximum precision run has no issues" = nrow(maximum$issues) == 0L,
  "the maximum precision run saves" = returns_paths(maximum$save()))
expect_identical(rawfile("rounded.csv"), expected, "the CSV at the maximum precision")
unlink("rounded.csv")
for (precision in fixture_strings(host_truth[["invalid_csv_precision"]])) {
  spec <- precision_study(precision)
  checked <- spec$check()
  failed <- spec$run()
  label <- paste("decimals:", precision)
  expect_identical(as.list(failed$issues[c("phase", "condition", "rule", "spec_paths")]),
    list(phase = "validation", condition = "invalid_field_type", rule = "CSV-6",
      spec_paths = list("output.decimals")),
    paste(label, "is refused"))
  stopifnot_case(label, "a refused run has no output" = is.null(failed$output),
    "the check finds the run's issue" = identical(checked$issues, failed$issues),
    "the context names the value" = grepl(paste0('"actual":', precision), failed$issues$context, fixed = TRUE),
    "the context names what is expected" =
      grepl(host_truth[["csv_precision_expected"]], failed$issues$context, fixed = TRUE),
    "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
    "nothing is written" = !file.exists("rounded.csv"))
}
for (case in list(c("-9223372036854775809", "rounded.csv", "invalid_field_type", "CSV-6"),
                  c("2", "rounded.parquet", "decimals_not_applicable", "CSV-6"))) {
  failed <- precision_study(case[[1L]], case[[2L]])$run()
  label <- sprintf("decimals %s for %s", case[[1L]], case[[2L]])
  expect_identical(as.list(failed$issues[c("condition", "rule", "spec_paths")]),
    list(condition = case[[3L]], rule = case[[4L]], spec_paths = list("output.decimals")),
    paste(label, "is refused"))
  stopifnot_case(label, "a refused run has no output" = is.null(failed$output),
    "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
    "nothing is written" = !file.exists(case[[2L]]))
  if (case[[1L]] == "-9223372036854775809") stopifnot_case(label,
    "the observations keep the exact value" = grepl('"actual":-9223372036854775809', observations(failed), fixed = TRUE),
    "the context keeps the exact value" = grepl('"actual":-9223372036854775809', failed$issues$context, fixed = TRUE))
}
for (invalid in fixture_strings(host_truth[["invalid_significant_digits"]])) {
  checked <- precision_study(invalid)$check()
  expect_identical(checked$issues$spec_paths, list("output.decimals"), paste("decimals:", invalid, "is one issue"))
}
invisible(precision_study("-1"))
writeLines(sub("VALUE + VALUE", "1 / 0", readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
failed <- yamaa_spec("spec.yaml")$run()
expect_identical(failed$issues$condition, "division_by_zero", "a failed rounded run")
stopifnot("a failed run has no output" = is.null(failed$output),
  "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
  "nothing is written" = !file.exists("rounded.csv"))
cat("public R CSV precision preserves unrounded tables, exact saved bytes and failure gates\n")
graph_dir <- file.path(work, "linked-artifact-refusal"); dir.create(graph_dir)
setwd(graph_dir)
writeLines(c('yamaa: "1.0"', "type: spec", "domain: PROD", "keys: [ID]",
  "input: {RAW: never-read.csv}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]",
  "output: {path: produced.csv, columns: [ID], decimals: 2}"), "producer.yaml")
writeLines(c('yamaa: "1.0"', "type: spec", "domain: CONS", "keys: [ID]",
  "input: {P: {path: produced.csv, schema: producer.yaml}}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]",
  "output: {path: consumer.csv, columns: [ID]}"), "spec.yaml")
spec <- yamaa_spec("spec.yaml")
checked <- refuse_activation(spec$check())
failed <- refuse_activation(spec$run())
# The producer never runs: the consumer reads its own linked path, and that
# stored artifact does not exist yet.
stopifnot("the consumer checks" = nrow(checked$issues) == 0L)
expect_identical(as.list(failed$issues[c("condition", "spec_paths")]),
  list(condition = "resource_path_missing", spec_paths = list("input.P.path")),
  "the consumer's stored artifact is missing")
stopifnot("the run failed" = grepl('"outcome":"failure"', observations(failed), fixed = TRUE),
  "the producer's input is never read" = !grepl('"path":"never-read.csv"', observations(failed), fixed = TRUE),
  "no source is read" = grepl('"source_reads":[]', observations(failed), fixed = TRUE))
stopifnot("a failed run has no output" = is.null(failed$output),
  "the producer writes nothing" = !file.exists("produced.csv"),
  "the consumer writes nothing" = !file.exists("consumer.csv"),
  "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"))
cat("public R functionless link reads its missing artifact without running the producer\n")
graph_document <- function(input, base, output, invalid = FALSE) c(
  'yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]", paste0("base: ", base),
  paste0("input: ", input), "columns:",
  paste0("  - {name: ID, type: int, label: Identifier, derivation: ", base, ".ID}"),
  "  - name: VALUE", "    type: str", "    label: Value", paste0("    derivation: ", base, ".VALUE"),
  if (invalid) '    verifications: [{matches: {pattern: "\\u00e9("}}]',
  paste0("output: {path: ", output, ", columns: [ID, VALUE]}"))
dir.create("parents")
writeLines(graph_document("{RAW: ../never-read.csv}", "RAW", "../leaf.csv", TRUE), "parents/base.yaml")
writeLines(c('yamaa: "1.0"', "type: spec", "parent: parents/base.yaml", "domain: LEAF"), "leaf.yaml")
writeLines(graph_document(host_truth[["aliased_leaf_reference"]], "P", "left.csv"), "left.yaml")
writeLines(graph_document(host_truth[["leaf_reference"]], "P", "right.csv"), "right.yaml")
writeLines(graph_document(host_truth[["diamond_reference"]], "L", "consumer.csv", TRUE), "spec.yaml")
for (attempt in 1:2) {
  # Linked producers supply output contracts only: their own findings are
  # never compiled, and only the entry is checked.
  checked <- refuse_activation(spec$check())
  same_issues(checked$issues, list(issue("invalid_regex", "TEXT-6",
    host_truth[["verification_pattern_path"]], '{"pattern":"\u00e9("}')),
    sprintf("linked check %d: the entry's own findings", attempt))
}
# A link types whatever file the consumer names; a path that differs from the
# producer's own output is admitted.
writeLines(graph_document(host_truth[["wrong_leaf_reference"]], "P", "consumer.csv"), "spec.yaml")
checked <- refuse_activation(spec$check())
stopifnot("a link may name another path than its producer's output" = nrow(checked$issues) == 0L)
writeLines(graph_document(host_truth[["incompatible_leaf_reference"]], "P", "consumer.csv"), "spec.yaml")
checked <- refuse_activation(spec$check())
expect_identical(as.list(checked$issues[c("condition", "rule", "spec_paths")]),
  list(condition = "redundant_field_type", rule = "INPUT-6", spec_paths = list("input.P.types.ID")),
  "a link's incompatible type")
stopifnot("linked checks write nothing" = !any(file.exists(fixture_strings(host_truth[["unread_graph_files"]]))))
cat("public R linked checks report only the entry with no study/publication effects\n")
# Declaring-file input/output provenance survives inheritance and publication.
local_paths <- file.path(work, "declaring-file-paths"); dir.create(local_paths)
setwd(local_paths)
for (name in fixture_strings(host_truth[["inherited_directories"]])) dir.create(name)
writeLines("[invalid configuration", "yamaa-project.yaml")
# A project without project functions names no host environment (D110).
writeLines(c('yamaa: "1.0"', "type: project", "adam: {specs: [../entry/spec.yaml]}"),
  "shared/project.yaml")
writeLines(c('yamaa: "1.0"', "type: spec", "parent: ../shared/base.yaml", "domain: CHILD"),
  "entry/spec.yaml")
writeLines(c('yamaa: "1.0"', "type: spec", "model: ADaM", "domain: BASE", "keys: [ID]",
  "input: {SRC: data.csv}",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  "output: {path: ../outputs/held.csv, columns: [ID]}"), "shared/base.yaml")
writeLines(c("ID", "999"), "entry/data.csv"); writeLines(c("ID", "7"), "shared/data.csv")
project <- yamaa_project("shared/project.yaml")
checked <- refuse_activation(project$check())
held <- refuse_activation(project$spec("CHILD")$run())
stopifnot("the inherited project checks" = nrow(checked$issues) == 0L,
  "the inherited run has no issues" = nrow(held$issues) == 0L,
  "the input is read beside the declaring file" = identical(held$output$ID, 7),
  "nothing is written before save" = !file.exists("outputs/held.csv"),
  "no configuration file is read" = !grepl("yamaa-project.yaml", observations(held), fixed = TRUE))
unlink(fixture_strings(host_truth[["inherited_source_files"]]))
writeLines(c("ID", "8"), "shared/data.csv")
stopifnot("the held run saves" = returns_paths(held$save()))
expect_identical(rawfile("outputs/held.csv"), charToRaw("ID\n7\n"), "the held run saves the bytes it read")
writeLines(c("ID", "7"), "shared/data.csv")
source <- paste0(normalizePath("shared", winslash = "/"), "/../shared/./data.csv")
output <- paste0(normalizePath("outputs", winslash = "/"), "/absolute.csv")
local_document <- function(input) c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  paste0("input: {SRC: '", input, "'}"), paste0("output: {path: '", output, "', columns: [ID]}"))
writeLines(local_document(source), "entry/spec.yaml")
absolute <- yamaa_spec("entry/spec.yaml")$run()
stopifnot("absolute paths run" = nrow(absolute$issues) == 0L,
  "an absolute input is read" = identical(absolute$output$ID, 7),
  "nothing is written before save" = !file.exists(output),
  "an absolute output saves" = returns_paths(absolute$save()))
expect_identical(rawfile(output), charToRaw("ID\n7\n"), "the absolute output's bytes")
unlink(output)
writeLines(c("version: '1.0'", "data_roots: [shared]"), "yamaa-project.yaml")
writeLines(c("ID", "999"), "data.csv")
unlink("entry/data.csv")
dir.create("shared/directory.csv")
for (input in c("data.csv", "../shared/directory.csv")) {
  writeLines(local_document(input), "entry/spec.yaml")
  failed <- yamaa_spec("entry/spec.yaml")$run()
  expect_identical(as.list(failed$issues[c("condition", "spec_paths")]),
    list(condition = if (input == "data.csv") "resource_path_missing" else "resource_path_not_regular_file",
      spec_paths = list("input.SRC.path")),
    paste("an input of", input))
  stopifnot_case(paste("an input of", input), "a refused run has no output" = is.null(failed$output),
    "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
    "nothing is written" = !file.exists(output))
}
cat("public R declaring-file inheritance, absolute paths, held bytes and no configuration fallback passed\n")
# Project codelists obey the project's own declaring-file path.
ct_paths <- file.path(work, "codelist-paths"); dir.create(ct_paths); setwd(ct_paths)
for (name in fixture_strings(host_truth[["terminology_directories"]])) dir.create(name)
writeLines("[ignored configuration", "yamaa-project.yaml")
# The specification declares its model (SUB-3), whose origin pairs SUB-15
# checks: an SDTM `Assigned` origin names its source.
writeLines(c('yamaa: "1.0"', "type: spec", "model: SDTM", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: data.csv, types: {ID: str}}}",
  paste0("columns: [{name: ID, type: str, derivation: SRC.ID, submission: ",
    "{codelist: IDENTIFIERS, origins: [{type: Assigned, source: Sponsor}]}}]"),
  "output: {path: ../outputs/ids.csv, columns: [ID]}"), "entry/spec.yaml")
writeLines(c("ID", "01"), "entry/data.csv")
terminology <- c('yamaa: "1.0"', "type: codelists",
  "codelists: [{id: IDENTIFIERS, name: Identifiers, items: [{value: '01'}]}]")
writeLines(terminology, "terminology/ids.yaml"); writeLines("[unrelated document", "entry/ids.yaml")
project <- yamaa_project("project/project.yaml")
for (written in c("../terminology/ids.yaml", normalizePath("terminology/ids.yaml", winslash = "/"))) {
  writeLines(c('yamaa: "1.0"', "type: project", paste0("codelists: ['", written, "']"),
    "sdtm: {specs: [../entry/spec.yaml]}"), "project/project.yaml")
  label <- paste("a codelist listed as", written)
  stopifnot_case(label, "the project checks" = nrow(project$check()$issues) == 0L)
  spec <- project$spec("TEST")
  checked <- spec$check()
  result <- spec$run()
  stopifnot_case(label, "the specification checks" = nrow(checked$issues) == 0L,
    "the run has no issues" = nrow(result$issues) == 0L,
    "the run reads its input" = identical(result$output$ID, "01"),
    "the codelist is verified" = grepl('"verifications":[{', observations(result), fixed = TRUE),
    "nothing is written before save" = !file.exists("outputs/ids.csv"),
    "the run saves" = returns_paths(result$save()),
    "the saved CSV is exact" = identical(rawfile("outputs/ids.csv"), charToRaw("ID\n01\n")))
  unlink("outputs/ids.csv")
}
writeLines(c('yamaa: "1.0"', "type: project", "codelists: [ids.yaml]",
  "sdtm: {specs: [../entry/spec.yaml]}"), "project/project.yaml")
writeLines(terminology, "ids.yaml"); writeLines(terminology, "entry/ids.yaml")
checked <- refuse_activation(project$check())$issues
runs <- refuse_activation(project$run())
# A listed file that cannot be read fails as any file a run reads, at its
# listing (D107).
expect_identical(as.list(checked[c("condition", "spec_paths")]),
  list(condition = "resource_path_missing", spec_paths = list("codelists[0]")),
  "a codelist missing beside the project")
stopifnot("the finding names the listed path" = grepl('"path":"ids.yaml"', checked$context, fixed = TRUE),
  "the refused project builds nothing" = identical(runs$names, character()),
  "the run carries the check's issue" = identical(runs$issues, checked),
  "nothing is written" = !file.exists("outputs/ids.csv"))
cat("public R relative/absolute project codelists and missing-file ownership passed\n")
setwd(previous)
unlink(work, recursive = TRUE)
cat("public R project static diagnostics, lock/test failure ordering and no data reads passed\n")

# Static submission-version admission opens neither listed specifications,
# linked documents, prospective outputs nor missing study data.
qualify_submission_versions <- function() {
  old <- getwd()
  directory <- tempfile("submission-version-"); dir.create(directory)
  on.exit({ setwd(old); unlink(directory, recursive = TRUE) }, add = TRUE)
  setwd(directory)
  project <- yamaa_project("project.yaml")
  for (family in c("sdtm", "adam", "send")) {
    guide <- switch(family, sdtm = "SDTMIG", adam = "ADaMIG", send = "SENDIG")
    model <- switch(family, sdtm = "SDTM", adam = "ADaM", send = "SEND")
    # A section lists only specifications of its model (SUB-3).
    writeLines(c('yamaa: "1.0"', "type: spec", paste0("model: ", model), "domain: TEST",
      "keys: [ID]", "input: {SRC: absent.csv}", "columns: [{name: ID, type: int, derivation: SRC.ID}]",
      "output: {path: result.csv, columns: [ID]}"), "spec.yaml")
    prefix <- c('yamaa: "1.0"', "type: project",
      "study: {id: STUDY, name: Study, description: Study, protocol_name: Protocol}",
      paste0(family, ":"),
      paste0("  standards: [{name: ", guide, ", version: '1.0', status: Final}]"),
      "  specs: [spec.yaml]", "  define: absent/define.xml")
    for (declared in list(NULL, "'1.0'", "'future package build'")) {
      field <- if (is.null(declared)) character() else paste0("  source_system_version: ", declared)
      writeLines(c(prefix, field), "project.yaml")
      stopifnot_case(sprintf("%s version %s", family, if (is.null(declared)) "absent" else declared),
        "an admitted version checks" = nrow(refuse_activation(project$check())$issues) == 0L)
    }
    for (declared in fixture_strings(host_truth[["invalid_source_system_versions"]])) {
      writeLines(c(prefix, paste0("  source_system_version: ", declared)), "project.yaml")
      issues <- refuse_activation(project$check())$issues
      expect_identical(issues$spec_paths, list(paste0(family, ".source_system_version")),
        sprintf("%s version %s is one issue", family, declared))
    }
    expect_identical(sort(list.files()), c("project.yaml", "spec.yaml"), paste(family, "checks write nothing"))
  }
  cat("public R submission versions are admitted before any artifact or study read\n")
}
qualify_submission_versions()

# A fresh directory with the negative-function-contract function file and R
# lock, and a project that lists one spec.
callback_study <- function(prefix) {
  work <- tempfile(prefix); dir.create(work); setwd(work); dir.create("r"); dir.create("functions")
  case <- file.path(root, "negative-function-contract")
  stopifnot("the lock is copied" = file.copy(file.path(case, "r", "renv.lock"), "r"),
    "the function file is copied" = file.copy(file.path(case, "functions", "project_value.yaml"), "functions"))
  writeLines(c("yamaa: '1.0'", "type: project", "functions: [functions/project_value.yaml]",
    "environments: {r: {lock: r/renv.lock}}", "adam: {specs: [spec.yaml]}"), "project.yaml")
  yamaa_project("project.yaml")
}

qualify_aggregate_callback <- function() {
  previous <- getwd()
  on.exit(setwd(previous))
  project <- callback_study("aggregate-callback-")
  spec <- paste(c(
    'yamaa: "1.0"', "type: spec", "model: ADaM", "domain: TEST", "input:",
    "  SRC: {path: input.csv, types: {ID: int, N: float}}",
    "  B: {path: input.csv, types: {ID: int, N: str}}",
    "base: SRC", "keys: [ID, N]", "intermediates:",
    "  - {id: CAP, dataset: SRC, key: {N: SRC.N}, order_by: [SRC.ID], keep: first}",
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    "  - {name: N, type: float, derivation: SRC.N}",
    "  - name: VALUE", "    type: float", "    derivation:", "      aggregate:",
    "        key: {ID: ID}", "        derivations:",
    "          BASE: {type: float, derivation: B.N}",
    "          V: {type: float, derivation: {function: {name: project_value, args: {x: CAP.N}}}}",
    "        expr: SUM(V)", "output: {path: output.csv, columns: [ID, N, VALUE]}"
  ), collapse = "\n")
  writeLines(spec, "spec.yaml"); writeLines(c("ID,N", "1,10", "1,30", "2,40"), "input.csv")
  result <- project$spec("TEST")$run()
  stopifnot("the aggregate run has no issues" = nrow(result$issues) == 0L)
  expect_identical(as.list(result$output[c("ID", "N", "VALUE")]),
    list(ID = c(1, 1, 2), N = c(10, 30, 40), VALUE = c(20, 60, 40)), "the aggregate callback's output")
  stopifnot("the aggregate run saves" = returns_paths(result$save()))
  expect_identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,20\n1,30,60\n2,40,40\n",
    "the aggregate callback's saved CSV")
  writeLines(sub(", order_by: [SRC.ID], keep: first", "", spec, fixed = TRUE), "spec.yaml")
  unlink("input.csv")
  issues <- refuse_activation(project$spec("TEST")$check())$issues
  stopifnot("an unkept intermediate refuses its derive step" =
    any(issues$condition == "invalid_derive_step" & issues$rule == "LOOKUP-22"))
  cat("public R aggregate callbacks preserve current-source types and kept record identity\n")
}
qualify_aggregate_callback()

qualify_row_callback_joins <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  project <- callback_study("row-callback-joins-")
  writeLines(c("ID,N", "1,10", "2,20", "3,30"), "a.csv")
  writeLines(c("ID,N", "2,40", "1,50"), "b.csv")
  for (driver in c("SRC", "B")) {
    other <- if (driver == "SRC") "B" else "SRC"
    # The driver's partner is read through an implicit join, on the key each
    # input declares (D23, D111).
    spec <- paste(c('yamaa: "1.0"', "type: spec", "model: ADaM", "domain: TEST", "input:",
      "  SRC: {path: a.csv, types: {ID: int, N: float}, key: [ID]}",
      "  B: {path: b.csv, types: {ID: int, N: float}, key: [ID]}",
      "base: SRC", "keys: [ID]", "columns:", "  - {name: ID, type: int}", "  - {name: VALUE, type: float}",
      "rows:", "  - id: records", paste0("    dataset: ", driver), "    derivations:",
      paste0("      ID: ", driver, ".ID"), paste0("      VALUE: {function: {name: project_value, args: {x: ", other, ".N}}}"),
      "output: {path: output.csv, columns: [ID, VALUE]}"), collapse="\n")
    writeLines(spec, "spec.yaml")
    result <- project$spec("TEST")$run()
    expected_id <- if (driver == "SRC") c(1, 2, 3) else c(2, 1)
    expected_value <- if (driver == "SRC") c(50, 40, NA_real_) else c(20, 10)
    expected_csv <- if (driver == "SRC") "ID,VALUE\n1,50\n2,40\n3,\n" else "ID,VALUE\n2,20\n1,10\n"
    label <- paste("row callbacks driven by", driver)
    stopifnot_case(label, "the run has no issues" = nrow(result$issues) == 0L)
    expect_identical(as.list(result$output[c("ID", "VALUE")]), list(ID = expected_id, VALUE = expected_value),
      paste(label, "join by completed keys"))
    stopifnot_case(label, "the run saves" = returns_paths(result$save()))
    expect_identical(rawToChar(rawfile("output.csv")), expected_csv, paste(label, "save the exact CSV"))
  }
  cat("public R row callbacks join primary and secondary source records by completed keys\n")
}
qualify_row_callback_joins()

qualify_intermediate_callback <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  project <- callback_study("intermediate-callback-")
  writeLines(c("ID,N", "1,10", "1,30", "2,40"), "a.csv")
  writeLines(c("ID,N", "1,one", "2,two"), "b.csv")
  writeLines(c('yamaa: "1.0"', "type: spec", "model: ADaM", "domain: TEST", "input:",
    "  SRC: {path: a.csv, types: {ID: int, N: float}}", "  B: {path: b.csv, types: {ID: int, N: str}}",
    "base: SRC", "keys: [ID, N]", "intermediates:",
    "  - {id: CAP, dataset: SRC, key: [ID], order_by: [SRC.N], keep: first}",
    "  - id: DON", "    dataset: B", "    key: [ID]", "    order_by: [B.ID]", "    keep: first",
    "    derivations:", "      BASE: CAP.N", "      VALUE: {function: {name: project_value, args: {x: BASE}}}",
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}", "  - {name: N, type: float, derivation: SRC.N}",
    "  - {name: VALUE, type: float, derivation: DON.VALUE}", "output: {path: output.csv, columns: [ID, N, VALUE]}"), "spec.yaml")
  result <- project$spec("TEST")$run()
  stopifnot("the intermediate run has no issues" = nrow(result$issues) == 0L)
  expect_identical(as.list(result$output[c("N", "VALUE")]), list(N = c(10, 30, 40), VALUE = c(10, 10, 40)),
    "the intermediate callback keeps prior values")
  stopifnot("the intermediate run saves" = returns_paths(result$save()))
  expect_identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,10\n1,30,10\n2,40,40\n",
    "the intermediate callback's saved CSV")
  cat("public R intermediate callbacks preserve prior values and selected donor field types\n")
}
qualify_intermediate_callback()

# Matching is linear: a nested repetition that cannot match is an ordinary
# verification failure, not an exhausted step budget.
qualify_catastrophic_pattern <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("catastrophic-pattern-"); dir.create(work); setwd(work)
  truth <- host_truth[["catastrophic_pattern"]]
  # A section lists only specifications of its model (SUB-3).
  specification <- sub("type: spec\n", "type: spec\nmodel: ADaM\n", truth$specification, fixed = TRUE)
  writeChar(specification, "spec.yaml", eos = NULL, useBytes = TRUE)
  writeChar(truth$source, "input.csv", eos = NULL, useBytes = TRUE)
  # A project without project functions names no host environment (D110).
  writeLines(c('yamaa: "1.0"', "type: project", "adam: {specs: [spec.yaml]}"), "project.yaml")
  specs <- list(alone = yamaa_spec("spec.yaml"), project = yamaa_project("project.yaml")$spec("TEST"))
  for (route in names(specs)) {
    failed <- specs[[route]]$run()
    label <- paste("a catastrophic pattern, specification", if (route == "alone") "alone" else "of a project")
    expect_identical(as.list(failed$issues[c("condition", "spec_paths")]),
      list(condition = truth$condition, spec_paths = list(unlist(truth$paths, use.names = FALSE))),
      paste(label, "fails the check"))
    context <- jsonlite::fromJSON(failed$issues$context[[1]], simplifyVector = FALSE)
    expect_context(context[names(truth$context)], truth$context, paste(label, "has the authored context"))
    stopifnot_case(label, "a failed run has no output" = is.null(failed$output),
      "its save signals yamaa_error" = inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
      "nothing is written" = !file.exists("output.csv"))
  }
  cat("public R catastrophic pattern fails the same check with and without a project\n")
}
qualify_catastrophic_pattern()
