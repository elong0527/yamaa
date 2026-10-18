# Lazy objects reuse what their first run compiled (#2821). A spec or project
# object's first run compiles and keeps its plans; a later run reuses them once
# every file they were compiled from is unchanged, and compiles again when one
# changed, so no run is stale. Inputs are read afresh by every run. Inputs and
# truth come from the shared plan-reuse host fixtures, which Python reads too.
library(yamaa)
host_fixture_root <- if (file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "plan-reuse")
root <- tempfile("yamaa-plan-reuse-"); dir.create(root)
root <- normalizePath(root, winslash = "/")

# A compiling run's private measurements hold the preparation phase; a reusing
# run's start at the run.
compiled <- function(run) {
  grepl('"phase":"preparation"', yamaa:::.run_observations(run, include_measurements = TRUE),
    fixed = TRUE)
}
# Files keep LF line ends on every platform.
write_text <- function(path, text) {
  dir.create(dirname(path), showWarnings = FALSE, recursive = TRUE)
  writeBin(charToRaw(enc2utf8(text)), path)
}
read_text <- function(path) rawToChar(readBin(path, "raw", n = file.info(path)$size))
refused <- function(code) inherits(tryCatch({ force(code); NULL }, yamaa_error = identity), "yamaa_error")
# Issue rows as the shared parity truth holds them: context as canonical JSON
# text, spans as doubles and routes as character.
issue_rows <- function(frame) {
  list(phase = frame$phase, condition = frame$condition, requirement = frame$requirement,
    spec_paths = frame$spec_paths, context = frame$context, source_span = frame$source_span,
    operand_route = frame$operand_route)
}
issue_truth <- function(expected) {
  text <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_character_ else issue[[field]], "")
  list(phase = text("phase"), condition = text("condition"), requirement = text("requirement"),
    spec_paths = lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
    context = enc2utf8(text("context")),
    source_span = lapply(expected, function(issue)
      if (is.null(issue$source_span)) NULL else as.double(unlist(issue$source_span))),
    operand_route = lapply(expected, function(issue)
      if (is.null(issue$operand_route)) NULL else as.character(unlist(issue$operand_route))))
}

# Run `spec` on one study's input and check its saved output or its issues.
spec_path <- file.path(root, "spec.yaml")
run_study <- function(spec, study) {
  write_text(file.path(root, "input.csv"), study$input_csv)
  output <- file.path(root, "output.csv")
  unlink(output)
  run <- spec$run()
  if (length(study$issues) == 0L) {
    stopifnot(nrow(run$issues) == 0L)
  } else if (!identical(issue_rows(run$issues), issue_truth(study$issues))) {
    str(issue_rows(run$issues)); str(issue_truth(study$issues))
    stop("issue rows differ from the shared plan-reuse truth")
  }
  if (is.null(study$output_csv)) {
    stopifnot(is.null(run$output), refused(run$save()), !file.exists(output))
  } else {
    run$save()
    stopifnot(identical(read_text(output), study$output_csv))
  }
  run
}

qualify_reuse <- function() {
  write_text(spec_path, truth$specification_yaml)
  spec <- yamaa_spec(spec_path)
  compiles <- vapply(truth$sequence, function(name) compiled(run_study(spec, truth$studies[[name]])),
    logical(1))
  stopifnot(identical(unname(compiles), c(TRUE, FALSE, FALSE)))
  # Another object for the same file compiles on its own first run.
  stopifnot(compiled(run_study(yamaa_spec(spec_path), truth$studies$A)))
  cat("later runs reuse the plan and read each input afresh\n")
}
qualify_reuse()

qualify_changed_specification <- function() {
  study <- truth$studies$A
  write_text(spec_path, truth$specification_yaml)
  spec <- yamaa_spec(spec_path)
  stopifnot(compiled(run_study(spec, study)))
  write_text(spec_path, truth$changed_specification_yaml)
  changed <- study
  changed$output_csv <- truth$changed_output_csv
  stopifnot(compiled(run_study(spec, changed)), !compiled(run_study(spec, changed)))
  # A specification the engine refuses keeps nothing: the run reports the
  # refusal, and once it is fixed the next run compiles it.
  write_text(spec_path, "schema_version: '1.0'\ndomain: TEST\n")
  failed <- spec$run()
  stopifnot(is.null(failed$output), nrow(failed$issues) > 0L)
  write_text(spec_path, truth$specification_yaml)
  stopifnot(compiled(run_study(spec, study)))
  cat("a changed specification compiles again and is never stale\n")
}
qualify_changed_specification()

qualify_changed_function_contract <- function() {
  studies <- system.file("project-original", package = "yamaa", mustWork = TRUE)
  stopifnot(file.copy(file.path(studies, "schema-functions"), root, recursive = TRUE))
  case <- file.path(root, "schema-functions")
  spec <- yamaa_project(file.path(case, "project.yaml"))$spec("TEST")
  expected <- read_text(file.path(case, "expected", "test.csv"))
  saved <- function(run) {
    run$save()
    read_text(file.path(case, "test.csv"))
  }
  first <- spec$run()
  stopifnot(compiled(first), identical(saved(first), expected), !compiled(spec$run()))
  # The function's default number of decimals changes in the environment
  # file: the run compiles the new contract and uses it.
  environment <- file.path(case, "r", "environment.yaml")
  original <- read_text(environment)
  write_text(environment, sub(truth$function_default$before, truth$function_default$after,
    original, fixed = TRUE))
  changed <- spec$run()
  stopifnot(compiled(changed), identical(saved(changed), truth$function_output_csv),
    !compiled(spec$run()))
  write_text(environment, original)
  restored <- spec$run()
  stopifnot(compiled(restored), identical(saved(restored), expected))
  # The project file changes: every plan it holds compiles again.
  project <- file.path(case, "project.yaml")
  write_text(project, paste0(read_text(project), "# reviewed\n"))
  stopifnot(compiled(spec$run()), !compiled(spec$run()))
  cat("a changed function contract or project file compiles again\n")
}
qualify_changed_function_contract()

qualify_project <- function() {
  study <- file.path(root, "project-study")
  dm <- paste0("schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\n",
    "input: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\n",
    "columns:\n",
    "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n",
    "  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\n",
    "output: {path: ../out/dm.csv, columns: [USUBJID, AGE]}\n")
  adsl <- paste0("schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\n",
    "input: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\n",
    "columns:\n",
    "  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n",
    "  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\n",
    "output: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n")
  write_text(file.path(study, "raw", "dm.csv"), "USUBJID,AGE\nS1,30\nS2,41\n")
  write_text(file.path(study, "sdtm", "dm.yaml"), dm)
  write_text(file.path(study, "adam", "adsl.yaml"), adsl)
  write_text(file.path(study, "project.yaml"), paste0("schema_version: '1.0'\n",
    "sdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adsl.yaml]\n"))
  dir.create(file.path(study, "out"))
  project <- yamaa_project(file.path(study, "project.yaml"))
  saved <- function() {
    runs <- project$run()
    stopifnot(identical(runs$names, c("DM", "ADSL")), nrow(runs$issues) == 0L)
    runs$save()
    read_text(file.path(study, "out", "adsl.csv"))
  }
  stopifnot(identical(saved(), "USUBJID,AGE\nS1,30\nS2,41\n"),
    identical(saved(), "USUBJID,AGE\nS1,30\nS2,41\n"))
  # Each run reads the inputs afresh.
  write_text(file.path(study, "raw", "dm.csv"), "USUBJID,AGE\nS1,31\nS2,41\n")
  stopifnot(identical(saved(), "USUBJID,AGE\nS1,31\nS2,41\n"))
  # A changed specification runs as changed.
  write_text(file.path(study, "adam", "adsl.yaml"),
    sub("[USUBJID, AGE]}", "[AGE, USUBJID]}", adsl, fixed = TRUE))
  stopifnot(identical(saved(), "AGE,USUBJID\n31,S1\n41,S2\n"),
    identical(saved(), "AGE,USUBJID\n31,S1\n41,S2\n"))
  # A spec object of the project keeps its own plan.
  spec <- project$spec("DM")
  stopifnot(identical(c(compiled(spec$run()), compiled(spec$run())), c(TRUE, FALSE)))
  cat("a project object reuses its plans and runs every change\n")
}
qualify_project()
unlink(root, recursive = TRUE)
