library(yamaa)
root <- system.file("project-original", package = "yamaa", mustWork = TRUE)
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
readtext <- function(path) readLines(path, warn = FALSE)
observations_native <- get(".domain_observations", asNamespace("yamaa"))
observations <- function(result) rawToChar(observations_native(result))
previous <- getwd()
work <- tempfile("public-environment-"); dir.create(work)
for (name in c("schema-functions", "schema-non-finite", "adam-adsl-bmi", "adam-advs-percentiles")) {
  stopifnot(file.copy(file.path(root, name), work, recursive = TRUE))
  case <- file.path(work, name); setwd(case)
  expected <- list.files("expected", pattern = "[.]csv$", full.names = TRUE)
  stopifnot(length(expected) == 1L, nrow(yamaa_check("spec.yaml", environment = "r/environment.yaml")$issues) == 0L)
  artifact <- basename(expected)
  for (attempt in 1:2) {
    result <- yamaa_domain("spec.yaml", environment = "r/environment.yaml")
    stopifnot(nrow(result$issues) == 0L, is.data.frame(result$output))
    before <- observations(result)
    stopifnot(grepl('"lock":"verified"', before, fixed = TRUE),
              grepl('"outcome":"passed"', before, fixed = TRUE),
              grepl('"artifacts":[]', before, fixed = TRUE), !file.exists(artifact))
    stopifnot(isTRUE(result$save()), identical(rawfile(artifact), rawfile(expected)))
    unlink(artifact)
  }
  cat(name, "public environment exact original CSV and repeated activation passed\n")
}
name <- "negative-function-contract"
stopifnot(file.copy(file.path(root, name), work, recursive = TRUE))
case <- file.path(work, name); setwd(case); unlink("input", recursive = TRUE)
expected <- list(phase = "validation", condition = "invalid_function_argument",
  requirement = "REQ-0700", spec_paths = list("columns.RESULT.derivation.function.args"),
  context = '{"argument":"x","cause":"missing_required_argument","function":"project_value"}')
for (result in list(yamaa_check("spec.yaml", environment = "r/environment.yaml"),
                    yamaa_domain("spec.yaml", environment = "r/environment.yaml"))) {
  stopifnot(identical(lapply(result$issues, identity), expected))
}
stopifnot(is.null(result$output), inherits(tryCatch(result$save(), error = identity), "yamaa_domain_error"))
# A valid static call still reads no study data and runs no activation.
writeLines(sub("args: {}", "args: {x: SOURCE.VALUE}", readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
stopifnot(nrow(yamaa_check("spec.yaml", environment = "r/environment.yaml")$issues) == 0L)
# Version disagreement precedes binding, tests and even a missing study input.
lock <- "r/renv.lock"
writeLines(sub('"Version": "1.0.0"', '"Version": "999999.0"', readtext(lock), fixed = TRUE), lock)
failed <- yamaa_domain("spec.yaml", environment = "r/environment.yaml")
stopifnot(identical(failed$issues$condition, "runtime_artifact_mismatch"),
          grepl('"source_reads":[]', observations(failed), fixed = TRUE),
          grepl('"bindings":[]', observations(failed), fixed = TRUE))
file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE)
# Independently failing test results collect completely on every attempted build.
env <- "r/environment.yaml"
text <- readtext(env)
text <- sub("result: 1.5", "result: 9.5", text, fixed = TRUE)
text <- sub("result: 0.0", "result: 9.0", text, fixed = TRUE)
writeLines(text, env)
for (attempt in 1:2) {
  failed <- yamaa_domain("spec.yaml", environment = env)
  stopifnot(identical(failed$issues$condition, rep("function_conformance_failed", 2L)),
            grepl('"source_reads":[]', observations(failed), fixed = TRUE))
}
# Captured syntax, rather than the selected host or basename, identifies the lock.
file.copy(file.path(root, name, "python/uv.lock"), lock, overwrite = TRUE)
checked <- yamaa_check("spec.yaml", environment = env)
stopifnot("project_environment_invalid" %in% checked$issues$condition)
# Codelist binding and function argument admission collect independent findings.
invisible(file.copy(file.path(root, name, "r/environment.yaml"), env, overwrite = TRUE))
writeLines(c(readtext(env), "codelists:", "  - codelists:", "      - id: IDENTIFIERS", "        name: Identifiers", "        items: [{value: '01'}]"), env)
invisible(file.copy(file.path(root, name, "r/renv.lock"), lock, overwrite = TRUE))
invisible(file.copy(file.path(root, name, "input"), case, recursive = TRUE))
text <- readtext(file.path(root, name, "spec.yaml"))
text <- sub("label: Identifier", "label: Identifier\n    submission: {codelist: IDENTIFIERS}", text, fixed = TRUE)
text <- sub("args: {}", "args: {x: SOURCE.VALUE}", text, fixed = TRUE)
writeLines(text, "spec.yaml")
stopifnot(nrow(yamaa_check("spec.yaml", environment = env)$issues) == 0L)
result <- yamaa_domain("spec.yaml", environment = env)
stopifnot(nrow(result$issues) == 0L, grepl('"verifications":[{', observations(result), fixed = TRUE))
text <- sub("codelist: IDENTIFIERS", "codelist: ABSENT", text, fixed = TRUE)
text <- sub("args: {x: SOURCE.VALUE}", "args: {}", text, fixed = TRUE)
writeLines(text, "spec.yaml"); unlink("input", recursive = TRUE)
checked <- yamaa_check("spec.yaml", environment = env)
stopifnot(setequal(checked$issues$requirement, c("REQ-0700", "REQ-0953")))
# Independently specified math truth is identical in column, row and nested case
# contexts. These declarations call no project function.
for (context in c("column", "row", "case")) {
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
  result <- yamaa_domain('spec.yaml', environment = 'r/environment.yaml')
  stopifnot(nrow(result$issues) == 0L, identical(result$output$RESULT, 9),
    isTRUE(result$save()), identical(rawfile('test.csv'), charToRaw('ID,RESULT\n01,9\n')))
}
setwd(previous)
cat("public R environment static diagnostics, lock/test failure ordering and no data reads passed\n")
