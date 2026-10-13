library(yamaa)
domain_file_native <- get("wrap__domain_file", asNamespace("yamaa"))
for (capabilities in list(list(), list(NULL, function(...) NULL, function(...) NULL))) {
  reply <- .Call(domain_file_native, charToRaw("absent-spec.yaml"),
    charToRaw("absent-environment.yaml"), charToRaw(as.character(getRversion())), capabilities)
  stopifnot(identical(reply$error, if (length(capabilities) == 0L)
    "invalid project host capabilities" else "invalid lock capability"))
}
root <- system.file("project-original", package = "yamaa", mustWork = TRUE)
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
readtext <- function(path) readLines(path, warn = FALSE)
observations_native <- get(".domain_observations", asNamespace("yamaa"))
observations <- function(result) rawToChar(observations_native(result))

qualify_unavailable_environment_root <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("environment-root-"); dir.create(work); setwd(work)
  base <- normalizePath(work, winslash = "/", mustWork = TRUE)
  for (name in "missing-environment.yaml") {
    cause <- "missing"
    expected <- list(phase = "validation", condition = "project_environment_missing",
      requirement = "REQ-0694", spec_paths = list("environment"),
      context = paste0('{"cause":"', cause, '","path":"', file.path(base, name), '"}'))
    for (content in list(NULL, "[entry must not be read")) {
      unlink("spec.yaml")
      if (!is.null(content)) writeLines(content, "spec.yaml")
      checked <- yamaa_check("spec.yaml", environment = name)
      failed <- yamaa_domain("spec.yaml", environment = name)
      stopifnot(identical(lapply(checked$issues, identity), expected),
        identical(lapply(failed$issues, identity), expected), is.null(failed$output),
        inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"))
      captured <- observations(failed)
      stopifnot(grepl('"lock":"not_requested"', captured, fixed = TRUE),
        grepl('"source_reads":[]', captured, fixed = TRUE),
        grepl('"bindings":[]', captured, fixed = TRUE),
        grepl('"callbacks":[]', captured, fixed = TRUE))
    }
  }
  unlink("spec.yaml")
  writeLines(c("schema_version: '1.0'", "language: r"), "environment.yaml")
  checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
  stopifnot(identical(checked$issues$condition, "engine_rejected"),
    identical(checked$issues$context, '{"code":"resource_missing","stage":"prepare"}'),
    identical(sort(list.files()), "environment.yaml"))
  cat("public R missing supplied environments retain exact REQ-0694 issues before entry/data/activation\n")
}
qualify_unavailable_environment_root()

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
text <- sub("label: Identifier", "label: Identifier\n    submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}", text, fixed = TRUE)
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
precision_dir <- file.path(work, "csv-precision"); dir.create(precision_dir)
setwd(precision_dir)
precision_study <- function(decimals = "2", path = "rounded.csv") {
  writeLines(c("schema_version: '1.0'", "language: r"), "environment.yaml")
  writeLines(c("ID,VALUE,INT", "1,0.125,7", "2,-0.125,-7", "3,2.675,0",
    "4,-0.004,9", "5,1.234,1", "6,2.345,2", "7,,3", "8,25,4"), "input.csv")
  precision <- if (is.null(decimals)) "" else paste0(", decimals: ", decimals)
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {SRC: {path: input.csv, types: {ID: int, VALUE: float, INT: int}}}",
    paste0("output: {path: ", path, ", columns: [ID, VALUE, DOUBLE, EMPTY, INT]", precision, "}"),
    "columns:", "  - {name: ID, type: int, derivation: SRC.ID}",
    "  - {name: VALUE, type: float, derivation: SRC.VALUE}",
    "  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}",
    "  - {name: EMPTY, type: str, derivation: {literal: ''}}",
    "  - {name: INT, type: int, derivation: SRC.INT}",
    "verifications: [{assert: {when: 'ID = 5', require: 'DOUBLE = 2.468'}}]"), "spec.yaml")
}
precision_study()
expected <- charToRaw(paste0('ID,VALUE,DOUBLE,EMPTY,INT\n1,0.13,0.25,"",7\n2,-0.13,-0.25,"",-7\n',
  '3,2.67,5.35,"",0\n4,0.00,-0.01,"",9\n5,1.23,2.47,"",1\n',
  '6,2.35,4.69,"",2\n7,,,"",3\n8,25.00,50.00,"",4\n'))
stopifnot(nrow(yamaa_check("spec.yaml", environment = "environment.yaml")$issues) == 0L)
rounded <- yamaa_domain("spec.yaml", environment = "environment.yaml")
stopifnot(nrow(rounded$issues) == 0L,
  identical(rounded$output$VALUE, c(0.125, -0.125, 2.675, -0.004, 1.234, 2.345, NA_real_, 25)),
  identical(rounded$output$DOUBLE, c(0.25, -0.25, 5.35, -0.008, 2.468, 4.69, NA_real_, 50)),
  !file.exists("rounded.csv"), isTRUE(rounded$save()), identical(rawfile("rounded.csv"), expected))
unlink("rounded.csv")
precision_study(NULL)
ordinary <- yamaa_domain("spec.yaml", environment = "environment.yaml")
stopifnot(nrow(ordinary$issues) == 0L, identical(ordinary$output, rounded$output))
precision_study("5000")
writeLines(c("ID,VALUE,INT", "1,0.125,7"), "input.csv")
large <- yamaa_domain("spec.yaml", environment = "environment.yaml")
expected <- charToRaw(paste0('ID,VALUE,DOUBLE,EMPTY,INT\n1,0.125', strrep("0", 4997L),
  ',0.25', strrep("0", 4998L), ',"",7\n'))
stopifnot(nrow(large$issues) == 0L, isTRUE(large$save()), identical(rawfile("rounded.csv"), expected))
unlink("rounded.csv")
precision_study("999999999999999999999999999999")
stopifnot(nrow(yamaa_check("spec.yaml", environment = "environment.yaml")$issues) == 0L)
failed <- yamaa_domain("spec.yaml", environment = "environment.yaml")
stopifnot(is.null(failed$output), identical(failed$issues$condition, "engine_rejected"),
  inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"), !file.exists("rounded.csv"))
for (case in list(c("-9223372036854775809", "rounded.csv", "invalid_field_type", "REQ-0744"),
                  c("2", "rounded.parquet", "decimals_not_applicable", "REQ-0762"))) {
  precision_study(case[[1L]], case[[2L]])
  failed <- yamaa_domain("spec.yaml", environment = "environment.yaml")
  stopifnot(is.null(failed$output), identical(failed$issues$condition, case[[3L]]),
    identical(failed$issues$requirement, case[[4L]]),
    identical(failed$issues$spec_paths, list("output.decimals")),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"), !file.exists(case[[2L]]))
  if (case[[1L]] == "-9223372036854775809") stopifnot(grepl(
    '"actual":-9223372036854775809', observations(failed), fixed = TRUE))
}
for (invalid in c("true", "2.0", "'2'")) {
  precision_study(invalid)
  checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
  stopifnot(nrow(checked$issues) == 1L, identical(checked$issues$spec_paths, list("output.decimals")))
}
precision_study("-1")
writeLines(sub("VALUE + VALUE", "1 / 0", readtext("spec.yaml"), fixed = TRUE), "spec.yaml")
failed <- yamaa_domain("spec.yaml", environment = "environment.yaml")
stopifnot(is.null(failed$output), identical(failed$issues$condition, "division_by_zero"),
  inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"), !file.exists("rounded.csv"))
cat("public R CSV precision preserves unrounded tables, exact saved bytes and failure gates\n")
graph_dir <- file.path(work, "producer-metadata-refusal"); dir.create(graph_dir)
setwd(graph_dir)
writeLines(c("schema_version: '1.0'", "language: r"), "environment.yaml")
writeLines(c("schema_version: '1.0'", "domain: PROD", "keys: [ID]",
  "input: {RAW: never-read.csv}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]",
  "output: {path: produced.csv, columns: [ID], decimals: 2}"), "producer.yaml")
writeLines(c("schema_version: '1.0'", "domain: CONS", "keys: [ID]",
  "input: {P: {path: produced.csv, schema: producer.yaml}}",
  "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]",
  "output: {path: consumer.csv, columns: [ID]}"), "spec.yaml")
checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
failed <- yamaa_domain("spec.yaml", environment = "environment.yaml")
stopifnot(nrow(checked$issues) == 0L,
  identical(failed$issues$condition, "resource_path_missing"),
  identical(failed$issues$spec_paths, list("input.RAW.path")),
  grepl('"outcome":"failure"', observations(failed), fixed = TRUE),
  grepl('"path":"never-read.csv"', observations(failed), fixed = TRUE))
stopifnot(is.null(failed$output), !file.exists("produced.csv"), !file.exists("consumer.csv"),
  inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"))
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
writeLines(graph_document("{P: {path: leaf.csv, schema: './leaf.yaml'}}", "P", "left.csv"), "left.yaml")
writeLines(graph_document("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "right.csv"), "right.yaml")
writeLines(graph_document("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml}, Q: {path: right.csv, schema: './right.yaml'}}", "L", "consumer.csv", TRUE), "spec.yaml")
for (attempt in 1:2) {
  checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
  expected_context <- vapply(c("spec.yaml", "leaf.yaml"), function(source) paste0(
    '{', if (source == "leaf.yaml") paste0('"declaring_sources":["', normalizePath("parents/base.yaml", winslash = "/"), '"],'),
    '"entry":"', normalizePath("spec.yaml", winslash = "/"), '","pattern":"\u00e9(","source":"',
    normalizePath(source, winslash = "/"), '"}'), character(1), USE.NAMES = FALSE)
  stopifnot(identical(checked$issues$phase, rep("validation", 2L)),
    identical(checked$issues$condition, rep("invalid_regex", 2L)),
    identical(checked$issues$requirement, rep("REQ-0827", 2L)),
    identical(checked$issues$spec_paths, rep(list("columns.VALUE.verifications[0].matches.pattern"), 2L)),
    identical(checked$issues$context, expected_context))
}
writeLines(graph_document("{P: {path: wrong.csv, schema: leaf.yaml}}", "P", "consumer.csv"), "spec.yaml")
checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
stopifnot(identical(checked$issues$condition, "producer_output_path_mismatch"),
  identical(checked$issues$requirement, "REQ-0534"),
  identical(checked$issues$spec_paths, list(c("input.P.path", "input.P.schema"))))
writeLines(graph_document("{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}", "P", "consumer.csv"), "spec.yaml")
checked <- yamaa_check("spec.yaml", environment = "environment.yaml")
stopifnot(identical(checked$issues$condition, "redundant_field_type"),
  identical(checked$issues$requirement, "REQ-0523"),
  identical(checked$issues$spec_paths, list("input.P.types.ID")),
  !any(file.exists(c("never-read.csv", "leaf.csv", "left.csv", "right.csv", "consumer.csv", "wrong.csv"))))
cat("public R original inherited diamond checks are complete with no study/publication effects\n")
# Declaring-file input/output provenance survives inheritance and publication.
local_paths <- file.path(work, "declaring-file-paths"); dir.create(local_paths)
setwd(local_paths)
for (name in c("entry", "shared", "outputs")) dir.create(name)
writeLines("[invalid configuration", "yamaa-project.yaml")
writeLines(c("schema_version: '1.0'", "language: r"), "shared/environment.yaml")
writeLines(c("schema_version: '1.0'", "parents: ../shared/base.yaml", "domain: CHILD"), "entry/spec.yaml")
writeLines(c("schema_version: '1.0'", "domain: BASE", "keys: [ID]", "input: {SRC: data.csv}",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  "output: {path: ../outputs/held.csv, columns: [ID]}"), "shared/base.yaml")
writeLines(c("ID", "999"), "entry/data.csv"); writeLines(c("ID", "7"), "shared/data.csv")
checked <- yamaa_check("entry/spec.yaml", environment = "shared/environment.yaml")
held <- yamaa_domain("entry/spec.yaml", environment = "shared/environment.yaml")
stopifnot(nrow(checked$issues) == 0L, nrow(held$issues) == 0L,
  identical(as.character(held$output$ID), "7"), !file.exists("outputs/held.csv"),
  !grepl("yamaa-project.yaml", observations(held), fixed = TRUE))
unlink(c("entry/spec.yaml", "shared/base.yaml", "shared/environment.yaml"))
writeLines(c("ID", "8"), "shared/data.csv")
stopifnot(isTRUE(held$save()), identical(rawfile("outputs/held.csv"), charToRaw("ID\n7\n")))
writeLines(c("schema_version: '1.0'", "language: r"), "environment.yaml")
writeLines(c("ID", "7"), "shared/data.csv")
source <- paste0(normalizePath("shared", winslash = "/"), "/../shared/./data.csv")
output <- paste0(normalizePath("outputs", winslash = "/"), "/absolute.csv")
local_document <- function(input) c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]",
  paste0("input: {SRC: '", input, "'}"), paste0("output: {path: '", output, "', columns: [ID]}"))
writeLines(local_document(source), "entry/spec.yaml")
absolute <- yamaa_domain("entry/spec.yaml", environment = "environment.yaml")
stopifnot(nrow(absolute$issues) == 0L, identical(as.character(absolute$output$ID), "7"),
  !file.exists(output), isTRUE(absolute$save()), identical(rawfile(output), charToRaw("ID\n7\n")))
unlink(output)
writeLines(c("version: '1.0'", "data_roots: [shared]"), "yamaa-project.yaml")
writeLines(c("ID", "999"), "data.csv")
unlink("entry/data.csv")
dir.create("shared/directory.csv")
for (input in c("data.csv", "../shared/directory.csv")) {
  writeLines(local_document(input), "entry/spec.yaml")
  failed <- yamaa_domain("entry/spec.yaml", environment = "environment.yaml")
  stopifnot(is.null(failed$output), identical(failed$issues$condition,
    if (input == "data.csv") "resource_path_missing" else "resource_path_not_regular_file"),
    identical(failed$issues$spec_paths, list("input.SRC.path")),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"), !file.exists(output))
}
cat("public R declaring-file inheritance, absolute paths, held bytes and no configuration fallback passed\n")
# External codelists obey the environment's own declaring-file path.
ct_paths <- file.path(work, "codelist-paths"); dir.create(ct_paths); setwd(ct_paths)
for (name in c("entry", "environment", "terminology", "outputs")) dir.create(name)
writeLines("[ignored configuration", "yamaa-project.yaml")
writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
  "input: {SRC: {path: data.csv, types: {ID: str}}}",
  "columns: [{name: ID, type: str, derivation: SRC.ID, submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}}]",
  "output: {path: ../outputs/ids.csv, columns: [ID]}"), "entry/spec.yaml")
writeLines(c("ID", "01"), "entry/data.csv")
terminology <- "codelists: [{id: IDENTIFIERS, name: Identifiers, items: [{value: '01'}]}]"
writeLines(terminology, "terminology/ids.yaml"); writeLines("[unrelated document", "entry/ids.yaml")
for (written in c("../terminology/ids.yaml", normalizePath("terminology/ids.yaml", winslash = "/"))) {
  writeLines(c("schema_version: '1.0'", "language: r", paste0("codelists: ['", written, "']")), "environment/env.yaml")
  checked <- yamaa_check("entry/spec.yaml", environment = "environment/env.yaml")
  result <- yamaa_domain("entry/spec.yaml", environment = "environment/env.yaml")
  stopifnot(nrow(checked$issues) == 0L, nrow(result$issues) == 0L,
    identical(result$output$ID, "01"), grepl('"verifications":[{', observations(result), fixed = TRUE),
    !file.exists("outputs/ids.csv"), isTRUE(result$save()), identical(rawfile("outputs/ids.csv"), charToRaw("ID\n01\n")))
  unlink("outputs/ids.csv")
}
writeLines(c("schema_version: '1.0'", "language: r", "codelists: [ids.yaml]"), "environment/env.yaml")
writeLines(terminology, "ids.yaml"); writeLines(terminology, "entry/ids.yaml")
failed <- yamaa_domain("entry/spec.yaml", environment = "environment/env.yaml")
stopifnot(is.null(failed$output), nrow(failed$issues) > 0L,
  inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"), !file.exists("outputs/ids.csv"))
cat("public R relative/absolute external codelists and missing-file ownership passed\n")
setwd(previous)
cat("public R environment static diagnostics, lock/test failure ordering and no data reads passed\n")

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
  for (family in c("sdtm", "adam", "send")) {
    guide <- switch(family, sdtm = "SDTMIG", adam = "ADaMIG", send = "SENDIG")
    prefix <- c("schema_version: '1.0'", "language: r",
      "study: {id: STUDY, name: Study, description: Study, protocol_name: Protocol}",
      paste0(family, ":"),
      paste0("  standard: {name: ", guide, ", version: '1.0', status: Final}"),
      "  specs: [absent-spec.yaml]", "  define: absent/define.xml")
    for (declared in list(NULL, "'1.0'", "'future package build'")) {
      field <- if (is.null(declared)) character() else paste0("  source_system_version: ", declared)
      writeLines(c(prefix, field), "environment.yaml")
      stopifnot(nrow(yamaa_check("spec.yaml", environment = "environment.yaml")$issues) == 0L)
    }
    for (declared in c("null", "true", "1", "1.5", "''")) {
      writeLines(c(prefix, paste0("  source_system_version: ", declared)), "environment.yaml")
      issues <- yamaa_check("spec.yaml", environment = "environment.yaml")$issues
      stopifnot(nrow(issues) == 1L,
        paste0(family, ".source_system_version") %in% issues$spec_paths[[1]])
    }
    stopifnot(identical(sort(list.files()), c("environment.yaml", "spec.yaml")))
  }
}
qualify_submission_versions()

qualify_aggregate_callback <- function() {
  previous <- getwd()
  on.exit(setwd(previous))
  work <- tempfile("aggregate-callback-"); dir.create(work)
  setwd(work); dir.create("r")
  stopifnot(file.copy(file.path(root, "negative-function-contract", "r", c("environment.yaml", "renv.lock")), "r"))
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
  result <- yamaa_domain("spec.yaml", environment = "r/environment.yaml")
  stopifnot(nrow(result$issues) == 0L,
    identical(as.character(result$output$ID), c("1", "1", "2")),
    identical(result$output$N, c(10, 30, 40)),
    identical(result$output$VALUE, c(20, 60, 40)), result$save(), result$save(),
    identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,20\n1,30,60\n2,40,40\n"))
  writeLines(sub(", order_by: [SRC.ID], keep: first", "", spec, fixed = TRUE), "spec.yaml")
  unlink("input.csv")
  issues <- yamaa_check("spec.yaml", environment = "r/environment.yaml")$issues
  stopifnot(any(issues$condition == "invalid_derive_step" & issues$requirement == "REQ-1242"))
  cat("public R aggregate callbacks preserve current-source types and kept record identity\n")
}
qualify_aggregate_callback()

qualify_row_callback_joins <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("row-callback-joins-"); dir.create(work); setwd(work); dir.create("r")
  stopifnot(file.copy(file.path(root, "negative-function-contract", "r", c("environment.yaml", "renv.lock")), "r"))
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
    result <- yamaa_domain("spec.yaml", environment="r/environment.yaml")
    expected_id <- if (driver == "SRC") c("1", "2", "3") else c("2", "1")
    expected_value <- if (driver == "SRC") c(50, 40, NA_real_) else c(20, 10)
    expected_csv <- if (driver == "SRC") "ID,VALUE\n1,50\n2,40\n3,\n" else "ID,VALUE\n2,20\n1,10\n"
    stopifnot(nrow(result$issues) == 0L, identical(as.character(result$output$ID), expected_id),
      identical(result$output$VALUE, expected_value), result$save(), result$save(),
      identical(rawToChar(rawfile("output.csv")), expected_csv))
  }
  cat("public R row callbacks join primary and secondary source records by completed keys\n")
}
qualify_row_callback_joins()

qualify_intermediate_callback <- function() {
  previous <- getwd(); on.exit(setwd(previous))
  work <- tempfile("intermediate-callback-"); dir.create(work); setwd(work); dir.create("r")
  stopifnot(file.copy(file.path(root, "negative-function-contract", "r", c("environment.yaml", "renv.lock")), "r"))
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
  result <- yamaa_domain("spec.yaml", environment="r/environment.yaml")
  stopifnot(nrow(result$issues)==0L, identical(result$output$N,c(10,30,40)),
    identical(result$output$VALUE,c(10,10,40)), result$save(), result$save(),
    identical(rawToChar(rawfile("output.csv")), "ID,N,VALUE\n1,10,10\n1,30,10\n2,40,40\n"))
  cat("public R intermediate callbacks preserve prior values and selected donor field types\n")
}
qualify_intermediate_callback()
