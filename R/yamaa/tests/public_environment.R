# Public installed projects and their environments against independent truth.
# A project file links one environment file per host; each host reads only its
# own entry, and project functions, codelists and locks reach a specification
# only through yamaa_project(path)$spec(name) or $run().
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-environment")
namespace <- asNamespace("yamaa")
spec_run_native <- get("wrap__spec_run", namespace)
for (capabilities in list(list(), list(NULL, function(...) NULL, function(...) NULL))) {
  reply <- .Call(spec_run_native, charToRaw("absent-spec.yaml"), NULL,
    charToRaw(as.character(getRversion())), capabilities)
  stopifnot(is.null(reply$value), identical(reply$error$message, if (length(capabilities) == 0L)
    "invalid project host capabilities" else "invalid lock capability"),
    nrow(reply$error$issues) == 0L)
}
root <- system.file("project-original", package = "yamaa", mustWork = TRUE)
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
readtext <- function(path) readLines(path, warn = FALSE)
observations <- function(run) yamaa:::.run_observations(run)
here <- function(name) file.path(normalizePath(getwd(), winslash = "/"), name)
# One issue row as a list of plain R values, for exact comparison.
row_of <- function(frame, index = 1L) lapply(frame, function(column) column[[index]])
issue <- function(condition, requirement, spec_paths, context, phase = "validation")
  list(phase = phase, condition = condition, requirement = requirement, spec_paths = spec_paths,
    context = context, source_span = NULL, operand_route = NULL)
same_issues <- function(frame, expected) {
  actual <- lapply(seq_len(nrow(frame)), function(index) row_of(frame, index))
  if (!identical(actual, expected)) {
    str(actual); str(expected)
    stop("issue rows differ from the exact authored tuples")
  }
}
refuse_activation <- function(code) {
  original <- get(".yamaa_locked_host_capabilities", namespace)
  replace <- function(value) {
    unlockBinding(".yamaa_locked_host_capabilities", namespace)
    assign(".yamaa_locked_host_capabilities", value, namespace)
    lockBinding(".yamaa_locked_host_capabilities", namespace)
  }
  on.exit(replace(original))
  refuse <- function(...) stop("unexpected activation")
  replace(function() list(verify = refuse, resolve = refuse))
  force(code)
}
# Project problems: issues of $check() and of a run that built nothing.
refused_project <- function(project, expected) {
  same_issues(project$check()$issues, expected)
  runs <- project$run()
  stopifnot(identical(runs$names, character()))
  same_issues(runs$issues, expected)
  error <- tryCatch(runs$save(), yamaa_error = identity)
  stopifnot(inherits(error, "yamaa_error"))
  same_issues(error$issues, expected)
  error <- tryCatch(project$spec("TEST"), yamaa_error = identity)
  stopifnot(inherits(error, "yamaa_error"))
  same_issues(error$issues, expected)
}
function_spec <- c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: input.csv, types: {VALUE: float}}}", "columns:",
  "  - {name: ID, type: str, derivation: SRC.ID}",
  "  - {name: VALUE, type: float, derivation: {function: {name: project_value, args: {x: SRC.VALUE}}}}",
  "output: {path: output.csv, columns: [ID, VALUE]}")
adam_project <- function(environment = "r/environment.yaml", specs = "spec.yaml") c(
  "schema_version: '1.0'", paste0("environments: {r: ", environment, "}"),
  paste0("adam: {specs: [", specs, "]}"))

qualify_metadata_failure_locations <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("metadata-locations-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(c("schema_version: '1.0'", "lock: missing.lock", "functions: {id: missing-function.yaml}"),
    "environment.yaml")
  writeLines(c("schema_version: '1.0'", "environments: {python: environment.yaml, r: environment.yaml}",
    "codelists: [missing-codelists.yaml]", "adam: {specs: [absent-spec.yaml]}"), "project.yaml")
  entry <- here("project.yaml")
  expected <- Map(function(declaration, path) issue("project_environment_invalid", "REQ-0695",
    declaration, paste0('{"cause":"missing","entry":"', entry, '","environment_path":"', declaration,
      '","path":"', path, '"}')),
    c("lock", "functions.id", "codelists[0]"),
    c("missing.lock", "missing-function.yaml", "missing-codelists.yaml"))
  refuse_activation(refused_project(yamaa_project("project.yaml"), unname(expected)))
  stopifnot(identical(sort(list.files()), c("environment.yaml", "project.yaml")))
  cat("public R project metadata failures name each declared input before any entry or activation\n")
}
qualify_metadata_failure_locations()

qualify_unavailable_environment_file <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("environment-file-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(c("schema_version: '1.0'",
    "environments: {python: missing-environment.yaml, r: missing-environment.yaml}",
    "adam: {specs: [spec.yaml]}"), "project.yaml")
  expected <- list(issue("project_environment_invalid", "REQ-0695", "environments.r",
    paste0('{"cause":"missing","entry":"', here("project.yaml"),
      '","environment_path":"environments.r","path":"missing-environment.yaml"}')))
  for (content in list(NULL, "[entry must not be read")) {
    unlink("spec.yaml")
    if (!is.null(content)) writeLines(content, "spec.yaml")
    refuse_activation(refused_project(yamaa_project("project.yaml"), expected))
  }
  stopifnot(identical(sort(list.files()), c("project.yaml", "spec.yaml")))
  cat("public R missing environment files refuse the project before entry preparation\n")
}
qualify_unavailable_environment_file()

qualify_own_environment_entry <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("own-environment-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  setwd(work)
  writeLines(function_spec, "spec.yaml")
  # The Python entry names a file that does not exist; R never reads it, so
  # a function call reports the missing R environment.
  writeLines(c("schema_version: '1.0'", "environments: {python: python/absent.yaml}",
    "adam: {specs: [spec.yaml]}"), "project.yaml")
  project <- yamaa_project("project.yaml")
  context <- '"cause":"undeclared","function":"project_value","language":"r"'
  path <- here("spec.yaml")
  refuse_activation({
    spec <- project$spec("TEST")
    for (result in list(spec$check(), spec$run())) {
      same_issues(result$issues, list(issue("project_environment_missing", "REQ-0694",
        "columns.VALUE.derivation.function.name", paste0("{", context, "}"))))
    }
    # A project result names the specification each issue belongs to.
    same_issues(project$check()$issues, list(issue("project_environment_missing", "REQ-0694",
      "columns.VALUE.derivation.function.name",
      paste0('{"cause":"undeclared","entry":"', path,
        '","function":"project_value","language":"r","source":"', path, '"}'))))
    # The same file alone has no project, so the function is unknown.
    same_issues(yamaa_spec("spec.yaml")$check()$issues, list(issue("unknown_project_function",
      "REQ-0698", "columns.VALUE.derivation.function.name", '{"function":"project_value"}')))
  })
  stopifnot(identical(sort(list.files()), c("project.yaml", "spec.yaml")))
  cat("public R reads only its own environments entry\n")
}
qualify_own_environment_entry()

qualify_static_check_without_study_inputs <- function() {
  work <- tempfile("static-check-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  # The caller's path may be any UTF-8 text; the project file itself is ASCII.
  study <- file.path(work, "\u00e9tude"); dir.create(study)
  for (name in fixture_strings(host_truth[["original_case_names"]])) {
    stopifnot(file.copy(file.path(root, name), study, recursive = TRUE))
    case <- file.path(study, name)
    unlink(file.path(case, "input"), recursive = TRUE)
    project <- yamaa_project(file.path(case, "project.yaml"))
    checked <- refuse_activation(project$check())
    domain <- sub("^domain: ", "", grep("^domain: ", readtext(file.path(case, "spec.yaml")), value = TRUE))
    spec <- refuse_activation(project$spec(domain)$check())
    stopifnot(nrow(checked$issues) == 0L, nrow(spec$issues) == 0L,
      length(list.files(case, pattern = "[.]csv$")) == 0L)
  }
  cat("public R project check validates original studies without study inputs or publication\n")
}
qualify_static_check_without_study_inputs()

qualify_functions_without_packaging_lock <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("unlocked-functions-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE), add = TRUE)
  stopifnot(file.copy(file.path(root, "adam-adsl-bmi"), work, recursive = TRUE))
  setwd(file.path(work, "adam-adsl-bmi"))
  environment <- "r/environment.yaml"
  writeLines(readtext(environment)[!grepl("^lock:", readtext(environment))], environment)
  unlink("r/renv.lock")
  expected <- list.files("expected", pattern = "[.]csv$", full.names = TRUE)
  project <- yamaa_project("project.yaml")
  stopifnot(length(expected) == 1L, nrow(project$check()$issues) == 0L)
  for (attempt in 1:2) {
    result <- project$spec("ADSL")$run()
    evidence <- observations(result)
    stopifnot(nrow(result$issues) == 0L, is.data.frame(result$output),
      grepl('"lock":"not_requested"', evidence, fixed = TRUE),
      grepl('"outcome":"passed"', evidence, fixed = TRUE),
      is.null(result$save()), identical(rawfile(basename(expected)), rawfile(expected)))
  }
  cat("public R functions without a packaging lock keep conformance and saved bytes\n")
}
qualify_functions_without_packaging_lock()

previous <- getwd()
work <- tempfile("public-environment-"); dir.create(work)
for (name in fixture_strings(host_truth[["original_case_names"]])) {
  stopifnot(file.copy(file.path(root, name), work, recursive = TRUE))
  case <- file.path(work, name); setwd(case)
  expected <- list.files("expected", pattern = "[.]csv$", full.names = TRUE)
  project <- yamaa_project("project.yaml")
  stopifnot(length(expected) == 1L, nrow(project$check()$issues) == 0L)
  runs <- project$run()
  domain <- runs$names
  stopifnot(nrow(runs$issues) == 0L, length(domain) == 1L, is.data.frame(runs[[domain]]$output))
  artifact <- basename(expected)
  for (attempt in 1:2) {
    result <- project$spec(domain)$run()
    stopifnot(nrow(result$issues) == 0L, identical(result$output, runs[[domain]]$output))
    before <- observations(result)
    stopifnot(grepl('"lock":"verified"', before, fixed = TRUE),
              grepl('"outcome":"passed"', before, fixed = TRUE),
              grepl('"artifacts":[]', before, fixed = TRUE), !file.exists(artifact))
    stopifnot(is.null(result$save()), identical(rawfile(artifact), rawfile(expected)))
    unlink(artifact)
  }
  stopifnot(is.null(runs$save()), identical(rawfile(artifact), rawfile(expected)))
  cat(name, "public project exact original CSV and repeated activation passed\n")
}
name <- "negative-function-contract"
stopifnot(file.copy(file.path(root, name), work, recursive = TRUE))
case <- file.path(work, name); setwd(case); unlink("input", recursive = TRUE)
project <- yamaa_project("project.yaml")
context <- '"argument":"x","cause":"missing_required_argument","function":"project_value"'
expected <- list(issue("invalid_function_argument", "REQ-0700",
  "columns.RESULT.derivation.function.args", paste0("{", context, "}")))
refuse_activation({
  spec <- project$spec("TEST")
  for (result in list(spec$check(), spec$run())) same_issues(result$issues, expected)
  # A project check names the specification each issue belongs to.
  same_issues(project$check()$issues, list(issue("invalid_function_argument", "REQ-0700",
    "columns.RESULT.derivation.function.args",
    paste0('{"argument":"x","cause":"missing_required_argument","entry":"', here("spec.yaml"),
      '","function":"project_value","source":"', here("spec.yaml"), '"}'))))
})
stopifnot(is.null(result$output), is.null(result$report),
  inherits(tryCatch(result$save(), error = identity), "yamaa_error"))
# A valid static call still reads no study data and runs no activation.
writeLines(sub("args: {}", "args: {x: SOURCE.VALUE}", readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
stopifnot(nrow(refuse_activation(project$spec("TEST")$check())$issues) == 0L)
# Version disagreement precedes binding, tests and even a missing study input.
lock <- "r/renv.lock"
writeLines(sub('"Version": "1.0.0"', '"Version": "999999.0"', readtext(lock), fixed = TRUE), lock)
failed <- project$spec("TEST")$run()
stopifnot(identical(failed$issues$condition, "runtime_artifact_mismatch"),
          grepl('"source_reads":[]', observations(failed), fixed = TRUE),
          grepl('"bindings":[]', observations(failed), fixed = TRUE))
invisible(file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE))
# Independently failing test results collect completely on every attempted run.
env <- "r/environment.yaml"
text <- readtext(env)
text <- sub("result: 1.5", "result: 9.5", text, fixed = TRUE)
text <- sub("result: 0.0", "result: 9.0", text, fixed = TRUE)
for (declared_lock in c(TRUE, FALSE)) {
  writeLines(if (declared_lock) text else text[!grepl("^lock:", text)], env)
  for (attempt in 1:2) {
    failed <- project$spec("TEST")$run()
    stopifnot(identical(failed$issues$condition, rep("function_conformance_failed", 2L)),
      grepl('"source_reads":[]', observations(failed), fixed = TRUE),
      grepl(if (declared_lock) '"lock":"verified"' else '"lock":"not_requested"',
        observations(failed), fixed = TRUE))
  }
}
invisible(file.copy(file.path(root, name, "r/environment.yaml"), env, overwrite = TRUE))
# Captured syntax, rather than the selected host or basename, identifies the lock.
invisible(file.copy(file.path(root, name, "python/uv.lock"), lock, overwrite = TRUE))
lock_kind <- function(environment) issue("project_environment_invalid", "REQ-0695", "lock",
  paste0('{"actual_lock_kind":"uv","entry":"', here("project.yaml"),
    '","environment_path":"lock","language":"r","reason":"lock_kind_mismatch","source":"',
    here(environment), '"}'))
same_issues(refuse_activation(project$check())$issues, list(lock_kind("r/environment.yaml")))
invisible(file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE))
# The environments key states the language: R reads the Python file as an R
# environment and refuses its lock and callable.
writeLines(c("schema_version: '1.0'", "environments: {python: python/environment.yaml, r: python/environment.yaml}",
  "adam: {specs: [spec.yaml]}"), "project.yaml")
checked <- refuse_activation(project$check())$issues
stopifnot(identical(row_of(checked), lock_kind("python/environment.yaml")),
  identical(checked$condition, rep("project_environment_invalid", 2L)),
  grepl('"reason":"invalid_callable"', checked$context[[2L]], fixed = TRUE))
# An environment file no longer states its language.
invisible(file.copy(file.path(root, name, "project.yaml"), "project.yaml", overwrite = TRUE))
writeLines(c("schema_version: '1.0'", "language: r", readtext(env)[-1L]), env)
checked <- refuse_activation(project$check())$issues
stopifnot(identical(checked$condition, "unknown_field"), identical(checked$requirement, "REQ-0285"),
  identical(checked$spec_paths, list("language")),
  grepl('"environment_path":"environments.r"', checked$context, fixed = TRUE))
invisible(file.copy(file.path(root, name, "r/environment.yaml"), env, overwrite = TRUE))
# Project codelists bind and function argument admission collect independent findings.
writeLines(c(readtext("project.yaml"), "codelists:", "  - codelists:", "      - id: IDENTIFIERS",
  "        name: Identifiers", "        items: [{value: '01'}]"), "project.yaml")
invisible(file.copy(file.path(root, name, "input"), case, recursive = TRUE))
text <- readtext(file.path(root, name, "spec.yaml"))
text <- sub("label: Identifier", "label: Identifier\n    submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}", text, fixed = TRUE)
text <- sub("args: {}", "args: {x: SOURCE.VALUE}", text, fixed = TRUE)
writeLines(text, "spec.yaml")
stopifnot(nrow(project$spec("TEST")$check()$issues) == 0L)
result <- project$spec("TEST")$run()
stopifnot(nrow(result$issues) == 0L, grepl('"verifications":[{', observations(result), fixed = TRUE))
text <- sub("codelist: IDENTIFIERS", "codelist: ABSENT", text, fixed = TRUE)
text <- sub("args: {x: SOURCE.VALUE}", "args: {}", text, fixed = TRUE)
writeLines(text, "spec.yaml"); unlink("input", recursive = TRUE)
checked <- refuse_activation(project$spec("TEST")$check())
stopifnot(setequal(checked$issues$requirement, c("REQ-0700", "REQ-0953")))
# Independently specified math truth is identical in column, row and nested case
# contexts. These declarations call no project function.
for (context in fixture_strings(host_truth[["quota_modes"]])) {
  target <- file.path(work, paste0("math-", context)); dir.create(target)
  stopifnot(file.copy(file.path(root, name), target, recursive = TRUE))
  setwd(file.path(target, name))
  text <- c('schema_version: "1.0"', 'domain: TEST', 'keys: [ID]',
    'input: {SOURCE: {path: input/source.csv, types: {VALUE: float}}}',
    'output: {path: test.csv, columns: [ID, RESULT]}', 'columns:',
    '  - {name: ID, type: str, label: Identifier, derivation: SOURCE.ID}',
    '  - name: RESULT', '    type: float', '    label: Result')
  expr <- 'EXP(0) + LN(1) + POWER(2, 3)'
  if (context == "column") text <- c(text, paste0('    derivation: {compute: {expr: "', expr, '"}}'))
  if (context == "row") text <- c(text, 'rows:', '  - id: records', '    dataset: SOURCE',
    paste0('    derivations: {RESULT: {compute: {expr: "', expr, '"}}}'))
  if (context == "case") text <- c(text, '    derivation:', '      case:',
    '        - when: "ID = \'01\'"', '          then:', '            case:',
    '              - when: "ID = \'01\'"',
    paste0('                then: {compute: {expr: "', expr, '"}}'),
    '              - otherwise: {literal: 99.0}', '        - otherwise: {literal: 98.0}')
  writeLines(text, 'spec.yaml')
  result <- yamaa_project('project.yaml')$spec('TEST')$run()
  stopifnot(nrow(result$issues) == 0L, identical(result$output$RESULT, 9),
    is.null(result$save()), identical(rawfile('test.csv'), charToRaw('ID,RESULT\n01,9\n')))
}
precision_dir <- file.path(work, "csv-precision"); dir.create(precision_dir)
setwd(precision_dir)
precision_study <- function(decimals = "2", path = "rounded.csv") {
  writeLines(c("ID,VALUE,INT", "1,0.125,7", "2,-0.125,-7", "3,2.675,0",
    "4,-0.004,9", "5,1.234,1", "6,2.345,2", "7,,3", "8,25,4"), "input.csv")
  precision <- if (is.null(decimals)) "" else paste0(", decimals: ", decimals)
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {SRC: {path: input.csv, types: {ID: int, VALUE: float, INT: int}}}",
    paste0("output: {path: ", path, host_truth[["callback_projection"]], precision, "}"),
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    "  - {name: VALUE, type: float, derivation: SRC.VALUE}",
    "  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}",
    "  - {name: EMPTY, type: str, derivation: {literal: ''}}",
    "  - {name: INT, type: int, derivation: SRC.INT}",
    "verifications: [{assert: {when: 'ID = 5', require: 'DOUBLE = 2.468'}}]"), "spec.yaml")
  yamaa_spec("spec.yaml")
}
spec <- precision_study()
expected <- fixture_bytes(host_truth[["rounded_callback_csv"]])
stopifnot(nrow(refuse_activation(spec$check())$issues) == 0L)
rounded <- refuse_activation(spec$run())
stopifnot(nrow(rounded$issues) == 0L,
  identical(rounded$output$VALUE, c(0.125, -0.125, 2.675, -0.004, 1.234, 2.345, NA_real_, 25)),
  identical(rounded$output$DOUBLE, c(0.25, -0.25, 5.35, -0.008, 2.468, 4.69, NA_real_, 50)),
  !file.exists("rounded.csv"), is.null(rounded$save()), identical(rawfile("rounded.csv"), expected))
unlink("rounded.csv")
ordinary <- precision_study(NULL)$run()
stopifnot(nrow(ordinary$issues) == 0L, identical(ordinary$output, rounded$output))
spec <- precision_study(host_truth[["csv_precision_maximum"]])
writeLines(c("ID,VALUE,INT", "1,0.125,7"), "input.csv")
maximum <- spec$run()
expected <- fixture_bytes(host_truth[["maximum_precision_csv"]])
stopifnot(nrow(maximum$issues) == 0L, is.null(maximum$save()), identical(rawfile("rounded.csv"), expected))
unlink("rounded.csv")
for (precision in fixture_strings(host_truth[["invalid_csv_precision"]])) {
  spec <- precision_study(precision)
  checked <- spec$check()
  failed <- spec$run()
  stopifnot(is.null(failed$output), nrow(failed$issues) == 1L,
    identical(checked$issues, failed$issues),
    identical(failed$issues$phase, "validation"),
    identical(failed$issues$condition, "invalid_field_type"),
    identical(failed$issues$requirement, "REQ-0744"),
    identical(failed$issues$spec_paths, list("output.decimals")),
    grepl(paste0('"actual":', precision), failed$issues$context, fixed = TRUE),
    grepl(host_truth[["csv_precision_expected"]], failed$issues$context, fixed = TRUE),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_error"), !file.exists("rounded.csv"))
}
for (case in list(c("-9223372036854775809", "rounded.csv", "invalid_field_type", "REQ-0744"),
                  c("2", "rounded.parquet", "decimals_not_applicable", "REQ-0762"))) {
  failed <- precision_study(case[[1L]], case[[2L]])$run()
  stopifnot(is.null(failed$output), identical(failed$issues$condition, case[[3L]]),
    identical(failed$issues$requirement, case[[4L]]),
    identical(failed$issues$spec_paths, list("output.decimals")),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_error"), !file.exists(case[[2L]]))
  if (case[[1L]] == "-9223372036854775809") stopifnot(grepl(
    '"actual":-9223372036854775809', observations(failed), fixed = TRUE),
    grepl('"actual":-9223372036854775809', failed$issues$context, fixed = TRUE))
}
for (invalid in fixture_strings(host_truth[["invalid_significant_digits"]])) {
  checked <- precision_study(invalid)$check()
  stopifnot(nrow(checked$issues) == 1L, identical(checked$issues$spec_paths, list("output.decimals")))
}
invisible(precision_study("-1"))
writeLines(sub("VALUE + VALUE", "1 / 0", readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
failed <- yamaa_spec("spec.yaml")$run()
stopifnot(is.null(failed$output), identical(failed$issues$condition, "division_by_zero"),
  inherits(tryCatch(failed$save(), error = identity), "yamaa_error"), !file.exists("rounded.csv"))
cat("public R CSV precision preserves unrounded tables, exact saved bytes and failure gates\n")
graph_dir <- file.path(work, "producer-metadata-refusal"); dir.create(graph_dir)
setwd(graph_dir)
writeLines(c("schema_version: '1.0'", "domain: PROD", "keys: [ID]",
  "input: {RAW: never-read.csv}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]",
  "output: {path: produced.csv, columns: [ID], decimals: 2}"), "producer.yaml")
writeLines(c("schema_version: '1.0'", "domain: CONS", "keys: [ID]",
  "input: {P: {path: produced.csv, schema: producer.yaml}}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]",
  "output: {path: consumer.csv, columns: [ID]}"), "spec.yaml")
spec <- yamaa_spec("spec.yaml")
checked <- refuse_activation(spec$check())
failed <- refuse_activation(spec$run())
stopifnot(nrow(checked$issues) == 0L,
  identical(failed$issues$condition, "resource_path_missing"),
  identical(failed$issues$spec_paths, list("input.RAW.path")),
  grepl('"outcome":"failure"', observations(failed), fixed = TRUE),
  grepl('"source_reads":[]', observations(failed), fixed = TRUE),
  # The entry never ran, so the producer graph returns no run report.
  is.null(failed$report))
stopifnot(is.null(failed$output), !file.exists("produced.csv"), !file.exists("consumer.csv"),
  inherits(tryCatch(failed$save(), error = identity), "yamaa_error"))
cat("public R selected functionless producer failure retains its actual source prefix\n")
graph_document <- function(input, base, output, invalid = FALSE) c(
  "schema_version: '1.0'", "domain: TEST", "keys: [ID]", paste0("base: ", base),
  paste0("input: ", input), "columns:",
  paste0("  - {name: ID, type: int, label: Identifier, derivation: ", base, ".ID}"),
  "  - name: VALUE", "    type: str", "    label: Value", paste0("    derivation: ", base, ".VALUE"),
  if (invalid) '    verifications: [{matches: {pattern: "\\u00e9("}}]',
  paste0("output: {path: ", output, ", columns: [ID, VALUE]}"))
dir.create("parents")
writeLines(graph_document("{RAW: ../never-read.csv}", "RAW", "../leaf.csv", TRUE), "parents/base.yaml")
writeLines(c("schema_version: '1.0'", "parents: [parents/base.yaml]", "domain: LEAF"), "leaf.yaml")
writeLines(graph_document(host_truth[["aliased_leaf_reference"]], "P", "left.csv"), "left.yaml")
writeLines(graph_document(host_truth[["leaf_reference"]], "P", "right.csv"), "right.yaml")
writeLines(graph_document(host_truth[["diamond_reference"]], "L", "consumer.csv", TRUE), "spec.yaml")
for (attempt in 1:2) {
  checked <- refuse_activation(spec$check())
  expected <- lapply(c("spec.yaml", "leaf.yaml"), function(source) issue("invalid_regex", "REQ-0827",
    host_truth[["verification_pattern_path"]], paste0(
    '{', if (source == "leaf.yaml") paste0('"declaring_sources":["', normalizePath("parents/base.yaml", winslash = "/"), '"],'),
    '"entry":"', normalizePath("spec.yaml", winslash = "/"), '","pattern":"é(","source":"',
    normalizePath(source, winslash = "/"), '"}')))
  same_issues(checked$issues, expected)
}
writeLines(graph_document(host_truth[["wrong_leaf_reference"]], "P", "consumer.csv"), "spec.yaml")
checked <- refuse_activation(spec$check())
stopifnot(identical(checked$issues$condition, "producer_output_path_mismatch"),
  identical(checked$issues$requirement, "REQ-0534"),
  identical(checked$issues$spec_paths, list(c("input.P.path", "input.P.schema"))))
writeLines(graph_document(host_truth[["incompatible_leaf_reference"]], "P", "consumer.csv"), "spec.yaml")
checked <- refuse_activation(spec$check())
stopifnot(identical(checked$issues$condition, "redundant_field_type"),
  identical(checked$issues$requirement, "REQ-0523"),
  identical(checked$issues$spec_paths, list("input.P.types.ID")),
  !any(file.exists(fixture_strings(host_truth[["unread_graph_files"]]))))
cat("public R original inherited diamond checks are complete with no study/publication effects\n")
# Declaring-file input/output provenance survives inheritance and publication.
local_paths <- file.path(work, "declaring-file-paths"); dir.create(local_paths)
setwd(local_paths)
for (name in fixture_strings(host_truth[["inherited_directories"]])) dir.create(name)
writeLines("[invalid configuration", "yamaa-project.yaml")
writeLines("schema_version: '1.0'", "shared/environment.yaml")
writeLines(c("schema_version: '1.0'", "environments: {python: environment.yaml, r: environment.yaml}",
  "adam: {specs: [../entry/spec.yaml]}"), "shared/project.yaml")
writeLines(c("schema_version: '1.0'", "parents: ../shared/base.yaml", "domain: CHILD"), "entry/spec.yaml")
writeLines(c("schema_version: '1.0'", "domain: BASE", "keys: [ID]", "input: {SRC: data.csv}",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  "output: {path: ../outputs/held.csv, columns: [ID]}"), "shared/base.yaml")
writeLines(c("ID", "999"), "entry/data.csv"); writeLines(c("ID", "7"), "shared/data.csv")
project <- yamaa_project("shared/project.yaml")
checked <- refuse_activation(project$check())
held <- refuse_activation(project$spec("CHILD")$run())
stopifnot(nrow(checked$issues) == 0L, nrow(held$issues) == 0L,
  identical(held$output$ID, 7), !file.exists("outputs/held.csv"),
  !grepl("yamaa-project.yaml", observations(held), fixed = TRUE))
unlink(fixture_strings(host_truth[["inherited_source_files"]]))
writeLines(c("ID", "8"), "shared/data.csv")
stopifnot(is.null(held$save()), identical(rawfile("outputs/held.csv"), charToRaw("ID\n7\n")))
writeLines(c("ID", "7"), "shared/data.csv")
source <- paste0(normalizePath("shared", winslash = "/"), "/../shared/./data.csv")
output <- paste0(normalizePath("outputs", winslash = "/"), "/absolute.csv")
local_document <- function(input) c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  paste0("input: {SRC: '", input, "'}"), paste0("output: {path: '", output, "', columns: [ID]}"))
writeLines(local_document(source), "entry/spec.yaml")
absolute <- yamaa_spec("entry/spec.yaml")$run()
stopifnot(nrow(absolute$issues) == 0L, identical(absolute$output$ID, 7),
  !file.exists(output), is.null(absolute$save()), identical(rawfile(output), charToRaw("ID\n7\n")))
unlink(output)
writeLines(c("version: '1.0'", "data_roots: [shared]"), "yamaa-project.yaml")
writeLines(c("ID", "999"), "data.csv")
unlink("entry/data.csv")
dir.create("shared/directory.csv")
for (input in c("data.csv", "../shared/directory.csv")) {
  writeLines(local_document(input), "entry/spec.yaml")
  failed <- yamaa_spec("entry/spec.yaml")$run()
  stopifnot(is.null(failed$output), identical(failed$issues$condition,
    if (input == "data.csv") "resource_path_missing" else "resource_path_not_regular_file"),
    identical(failed$issues$spec_paths, list("input.SRC.path")),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_error"), !file.exists(output))
}
cat("public R declaring-file inheritance, absolute paths, held bytes and no configuration fallback passed\n")
# Project codelists obey the project's own declaring-file path.
ct_paths <- file.path(work, "codelist-paths"); dir.create(ct_paths); setwd(ct_paths)
for (name in fixture_strings(host_truth[["terminology_directories"]])) dir.create(name)
writeLines("[ignored configuration", "yamaa-project.yaml")
writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: data.csv, types: {ID: str}}}",
  "columns: [{name: ID, type: str, derivation: SRC.ID, submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}}]",
  "output: {path: ../outputs/ids.csv, columns: [ID]}"), "entry/spec.yaml")
writeLines(c("ID", "01"), "entry/data.csv")
terminology <- "codelists: [{id: IDENTIFIERS, name: Identifiers, items: [{value: '01'}]}]"
writeLines(terminology, "terminology/ids.yaml"); writeLines("[unrelated document", "entry/ids.yaml")
project <- yamaa_project("project/project.yaml")
for (written in c("../terminology/ids.yaml", normalizePath("terminology/ids.yaml", winslash = "/"))) {
  writeLines(c("schema_version: '1.0'", paste0("codelists: ['", written, "']"),
    "sdtm: {specs: [../entry/spec.yaml]}"), "project/project.yaml")
  stopifnot(nrow(project$check()$issues) == 0L)
  spec <- project$spec("TEST")
  checked <- spec$check()
  result <- spec$run()
  stopifnot(nrow(checked$issues) == 0L, nrow(result$issues) == 0L,
    identical(result$output$ID, "01"), grepl('"verifications":[{', observations(result), fixed = TRUE),
    !file.exists("outputs/ids.csv"), is.null(result$save()), identical(rawfile("outputs/ids.csv"), charToRaw("ID\n01\n")))
  unlink("outputs/ids.csv")
}
writeLines(c("schema_version: '1.0'", "codelists: [ids.yaml]", "sdtm: {specs: [../entry/spec.yaml]}"),
  "project/project.yaml")
writeLines(terminology, "ids.yaml"); writeLines(terminology, "entry/ids.yaml")
checked <- refuse_activation(project$check())$issues
runs <- refuse_activation(project$run())
stopifnot(identical(checked$condition, "project_environment_invalid"),
  identical(checked$spec_paths, list("codelists[0]")),
  grepl('"path":"ids.yaml"', checked$context, fixed = TRUE),
  identical(runs$names, character()), identical(runs$issues, checked), !file.exists("outputs/ids.csv"))
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
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {SRC: absent.csv}", "columns: [{name: ID, type: int, derivation: SRC.ID}]",
    "output: {path: result.csv, columns: [ID]}"), "spec.yaml")
  project <- yamaa_project("project.yaml")
  for (family in c("sdtm", "adam", "send")) {
    guide <- switch(family, sdtm = "SDTMIG", adam = "ADaMIG", send = "SENDIG")
    prefix <- c("schema_version: '1.0'",
      "study: {id: STUDY, name: Study, description: Study, protocol_name: Protocol}",
      paste0(family, ":"),
      paste0("  standard: {name: ", guide, ", version: '1.0', status: Final}"),
      "  specs: [spec.yaml]", "  define: absent/define.xml")
    for (declared in list(NULL, "'1.0'", "'future package build'")) {
      field <- if (is.null(declared)) character() else paste0("  source_system_version: ", declared)
      writeLines(c(prefix, field), "project.yaml")
      stopifnot(nrow(refuse_activation(project$check())$issues) == 0L)
    }
    for (declared in fixture_strings(host_truth[["invalid_source_system_versions"]])) {
      writeLines(c(prefix, paste0("  source_system_version: ", declared)), "project.yaml")
      issues <- refuse_activation(project$check())$issues
      stopifnot(nrow(issues) == 1L,
        identical(issues$spec_paths, list(paste0(family, ".source_system_version"))))
    }
    stopifnot(identical(sort(list.files()), c("project.yaml", "spec.yaml")))
  }
  cat("public R submission versions are admitted before any artifact or study read\n")
}
qualify_submission_versions()

# A fresh directory with the negative-function-contract R environment and a
# project that lists one spec.
callback_study <- function(prefix) {
  work <- tempfile(prefix); dir.create(work); setwd(work); dir.create("r")
  stopifnot(file.copy(file.path(root, "negative-function-contract", "r", c("environment.yaml", "renv.lock")), "r"))
  writeLines(adam_project(), "project.yaml")
  yamaa_project("project.yaml")
}

qualify_aggregate_callback <- function() {
  previous <- getwd()
  on.exit(setwd(previous))
  project <- callback_study("aggregate-callback-")
  spec <- paste(c(
    "schema_version: '1.0'", "domain: TEST", "input:",
    "  SRC: {path: input.csv, types: {ID: int, N: float}}",
    "  B: {path: input.csv, types: {ID: int, N: str}}",
    "base: SRC", "keys: [ID, N]", "intermediates:",
    "  - {id: CAP, dataset: SRC, key: {N: SRC.N}, order_by: [SRC.ID], keep: first}",
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    "  - {name: N, type: float, derivation: SRC.N}",
    "  - name: VALUE", "    type: float", "    derivation:", "      aggregate:",
    "        key: {ID: ID}", "        derive:",
    "          - {name: BASE, type: float, derivation: B.N}",
    "          - {name: V, type: float, derivation: {function: {name: project_value, args: {x: CAP.N}}}}",
    "        expr: SUM(V)", "output: {path: output.csv, columns: [ID, N, VALUE]}"
  ), collapse = "\n")
  writeLines(spec, "spec.yaml"); writeLines(c("ID,N", "1,10", "1,30", "2,40"), "input.csv")
  result <- project$spec("TEST")$run()
  stopifnot(nrow(result$issues) == 0L,
    identical(result$output$ID, c(1, 1, 2)),
    identical(result$output$N, c(10, 30, 40)),
    identical(result$output$VALUE, c(20, 60, 40)), is.null(result$save()),
    identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,20\n1,30,60\n2,40,40\n"))
  writeLines(sub(", order_by: [SRC.ID], keep: first", "", spec, fixed = TRUE), "spec.yaml")
  unlink("input.csv")
  issues <- refuse_activation(project$spec("TEST")$check())$issues
  stopifnot(any(issues$condition == "invalid_derive_step" & issues$requirement == "REQ-1242"))
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
    spec <- paste(c("schema_version: '1.0'", "domain: TEST", "input:",
      "  SRC: {path: a.csv, types: {ID: int, N: float}}", "  B: {path: b.csv, types: {ID: int, N: float}}",
      "base: SRC", "keys: [ID]", "columns:", "  - {name: ID, type: int}", "  - {name: VALUE, type: float}",
      "rows:", "  - id: records", paste0("    dataset: ", driver), "    derivations:",
      paste0("      ID: ", driver, ".ID"), paste0("      VALUE: {function: {name: project_value, args: {x: ", other, ".N}}}"),
      "output: {path: output.csv, columns: [ID, VALUE]}"), collapse="\n")
    writeLines(spec, "spec.yaml")
    result <- project$spec("TEST")$run()
    expected_id <- if (driver == "SRC") c(1, 2, 3) else c(2, 1)
    expected_value <- if (driver == "SRC") c(50, 40, NA_real_) else c(20, 10)
    expected_csv <- if (driver == "SRC") "ID,VALUE\n1,50\n2,40\n3,\n" else "ID,VALUE\n2,20\n1,10\n"
    stopifnot(nrow(result$issues) == 0L, identical(result$output$ID, expected_id),
      identical(result$output$VALUE, expected_value), is.null(result$save()),
      identical(rawToChar(rawfile("output.csv")), expected_csv))
  }
  cat("public R row callbacks join primary and secondary source records by completed keys\n")
}
qualify_row_callback_joins()

qualify_intermediate_callback <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  project <- callback_study("intermediate-callback-")
  writeLines(c("ID,N", "1,10", "1,30", "2,40"), "a.csv")
  writeLines(c("ID,N", "1,one", "2,two"), "b.csv")
  writeLines(c("schema_version: '1.0'", "domain: TEST", "input:",
    "  SRC: {path: a.csv, types: {ID: int, N: float}}", "  B: {path: b.csv, types: {ID: int, N: str}}",
    "base: SRC", "keys: [ID, N]", "intermediates:",
    "  - {id: CAP, dataset: SRC, key: [ID], order_by: [SRC.N], keep: first}",
    "  - id: DON", "    dataset: B", "    key: [ID]", "    order_by: [B.ID]", "    keep: first",
    "    derivations:", "      BASE: CAP.N", "      VALUE: {function: {name: project_value, args: {x: BASE}}}",
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}", "  - {name: N, type: float, derivation: SRC.N}",
    "  - {name: VALUE, type: float, derivation: DON.VALUE}", "output: {path: output.csv, columns: [ID, N, VALUE]}"), "spec.yaml")
  result <- project$spec("TEST")$run()
  stopifnot(nrow(result$issues)==0L, identical(result$output$N,c(10,30,40)),
    identical(result$output$VALUE,c(10,10,40)), is.null(result$save()),
    identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,10\n1,30,10\n2,40,40\n"))
  cat("public R intermediate callbacks preserve prior values and selected donor field types\n")
}
qualify_intermediate_callback()

qualify_matching_budget <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("regex-budget-"); dir.create(work); setwd(work)
  truth <- host_truth[["regex_budget"]]
  writeChar(truth$specification, "spec.yaml", eos = NULL, useBytes = TRUE)
  writeChar(truth$source, "input.csv", eos = NULL, useBytes = TRUE)
  writeLines("schema_version: '1.0'", "environment.yaml")
  writeLines(adam_project("environment.yaml"), "project.yaml")
  for (spec in list(yamaa_spec("spec.yaml"), yamaa_project("project.yaml")$spec("TEST"))) {
    failed <- spec$run()
    stopifnot(is.null(failed$output), nrow(failed$issues) == 1L,
      identical(failed$issues$condition, truth$condition),
      identical(failed$issues$spec_paths[[1]], unlist(truth$paths, use.names = FALSE)))
    context <- jsonlite::fromJSON(failed$issues$context[[1]], simplifyVector = FALSE)
    stopifnot(context_equal(context[names(truth$context)], truth$context),
      inherits(tryCatch(failed$save(), error = identity), "yamaa_error"),
      !file.exists("output.csv"))
  }
  cat("public R matching budget has the same owner and bound with and without a project\n")
}
qualify_matching_budget()
