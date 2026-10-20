# Lazy objects reuse what their first run compiled (#2821). A spec or project
# object's first run compiles and keeps its plans; a later run reuses them while
# the object's paths still name the files they were compiled from and every one
# is unchanged, and compiles again otherwise, so no run is stale and none runs a
# study the paths no longer name (#3069). Inputs are read afresh by every run.
# Inputs and truth come from the shared plan-reuse host fixtures, which Python
# reads too.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped plan_reuse.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- if (file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "plan-reuse")
root <- tempfile("yamaa-plan-reuse-"); dir.create(root)
root <- normalizePath(root, winslash = "/")

# How a run got its plan, from its private measurements: compiled, refused or
# reused. The measurements close the observations, so their plan is the last
# one named. Match and slice bytes. On Windows, R's default regular expressions
# count UTF-16 code units, two for a character outside the Basic Multilingual
# Plane, while a character slice counts it once, so each one before the plan
# would move the slice one character on.
plan_of <- function(run) {
  text <- yamaa:::.run_observations(run, include_measurements = TRUE)
  found <- regmatches(text, gregexpr('"plan":"[a-z]+"', text, useBytes = TRUE))[[1L]]
  sub('^"plan":"([a-z]+)"$', "\\1", found[[length(found)]])
}
compiled <- function(run) !identical(plan_of(run), "reused")
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
  list(phase = frame$phase, condition = frame$condition, rule = frame$rule,
    spec_paths = frame$spec_paths, context = frame$context, source_span = frame$source_span,
    operand_route = frame$operand_route)
}
issue_truth <- function(expected) {
  text <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_character_ else issue[[field]], "")
  list(phase = text("phase"), condition = text("condition"), rule = text("rule"),
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
  studies <- system.file("project-base", package = "yamaa", mustWork = TRUE)
  stopifnot(file.copy(file.path(studies, "base-round"), root, recursive = TRUE))
  case <- file.path(root, "base-round")
  spec <- yamaa_project(file.path(case, "project.yaml"))$spec("TEST")
  expected <- read_text(file.path(case, "expected", "test.csv"))
  saved <- function(run) {
    run$save()
    read_text(file.path(case, "test.csv"))
  }
  first <- spec$run()
  stopifnot(compiled(first), identical(saved(first), expected), !compiled(spec$run()))
  # The function's default number of significant digits changes in the environment
  # file: the run compiles the new contract and uses it.
  environment <- file.path(case, "r", "environment.yaml")
  original <- read_text(environment)
  write_text(environment, sub("default: 3", "default: 2",
    sub("args: {x: 2.375}, result: 2.38", "args: {x: 2.375}, result: 2.4",
      original, fixed = TRUE), fixed = TRUE))
  changed <- spec$run()
  stopifnot(compiled(changed), identical(saved(changed), "ID,RESULT\n01,2.4\n02,-1.1\n03,\n"),
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

plain <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n",
  "input: {SRC: {path: input.csv, types: {ID: int}}}\n",
  "output: {path: out.csv, columns: [ID]}\n",
  "columns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n")
# Studies A and B with the same file names; B has one more row.
studies <- function(where) {
  for (name in c("a", "b")) {
    rows <- if (name == "a") "ID\n1\n2\n" else "ID\n1\n2\n3\n"
    write_text(file.path(where, name, "spec.yaml"), plain)
    write_text(file.path(where, name, "dm.yaml"),
      sub("out.csv", "dm.csv", sub("domain: TEST", "domain: DM", plain, fixed = TRUE), fixed = TRUE))
    write_text(file.path(where, name, "project.yaml"), "schema_version: '1.0'\nsdtm:\n  specs: [dm.yaml]\n")
    write_text(file.path(where, name, "input.csv"), rows)
  }
}

qualify_working_directory <- function() {
  where <- file.path(root, "moved")
  studies(where)
  previous <- setwd(file.path(where, "a"))
  on.exit(setwd(previous))
  spec <- yamaa_spec("spec.yaml")
  project <- yamaa_project("project.yaml")
  member <- project$spec("DM")
  saved <- function(run, study, name) {
    run$save()
    read_text(file.path(where, study, name))
  }
  a <- "ID\n1\n2\n"
  b <- "ID\n1\n2\n3\n"
  first <- spec$run()
  stopifnot(identical(plan_of(first), "compiled"), identical(saved(first, "a", "out.csv"), a),
    identical(plan_of(spec$run()), "reused"), identical(saved(project$run(), "a", "dm.csv"), a),
    identical(plan_of(member$run()), "compiled"), identical(plan_of(member$run()), "reused"))
  # The same spellings now name study B: each object builds B and leaves A's
  # saved files as they were.
  setwd(file.path(where, "b"))
  moved <- spec$run()
  stopifnot(identical(plan_of(moved), "compiled"), identical(saved(moved, "b", "out.csv"), b),
    identical(read_text(file.path(where, "a", "out.csv")), a), nrow(spec$check()$issues) == 0L,
    identical(saved(project$run(), "b", "dm.csv"), b),
    identical(read_text(file.path(where, "a", "dm.csv")), a))
  # `project$spec(name)` keeps its located specification, A's, with the
  # project's relative path, which now names B's project.
  stopifnot(identical(plan_of(member$run()), "compiled"), identical(plan_of(member$run()), "reused"))
  # Back in A, the spec object compiles A again: it keeps one plan.
  setwd(file.path(where, "a"))
  stopifnot(identical(plan_of(spec$run()), "compiled"))
  cat("relative paths after a working directory change build the new study\n")
}
qualify_working_directory()

qualify_link <- function() {
  if (.Platform$OS.type == "windows") return(invisible(NULL))
  where <- file.path(root, "linked")
  studies(where)
  link <- file.path(where, "study")
  stopifnot(file.symlink(file.path(where, "a"), link))
  spec <- yamaa_spec(file.path(link, "spec.yaml"))
  project <- yamaa_project(file.path(link, "project.yaml"))
  stopifnot(identical(plan_of(spec$run()), "compiled"), identical(plan_of(spec$run()), "reused"),
    nrow(project$run()[["DM"]]$output) == 2L)
  stopifnot(file.remove(link), file.symlink(file.path(where, "b"), link))
  moved <- spec$run()
  stopifnot(identical(plan_of(moved), "compiled"), nrow(moved$output) == 3L,
    nrow(project$run()[["DM"]]$output) == 3L)
  cat("a path through a retargeted link builds the study it now names\n")
}
qualify_link()

qualify_serialization <- function() {
  where <- file.path(root, "serialized")
  write_text(file.path(where, "spec.yaml"), plain)
  write_text(file.path(where, "input.csv"), "ID\n1\n")
  write_text(file.path(where, "dm.yaml"),
    sub("out.csv", "dm.csv", sub("domain: TEST", "domain: DM", plain, fixed = TRUE), fixed = TRUE))
  write_text(file.path(where, "project.yaml"), "schema_version: '1.0'\nsdtm:\n  specs: [dm.yaml]\n")
  spec <- yamaa_spec(file.path(where, "spec.yaml"))
  project <- yamaa_project(file.path(where, "project.yaml"))
  stopifnot(identical(plan_of(spec$run()), "compiled"))
  invisible(project$run())
  # A restored object brings an expired holder, so it starts with nothing
  # kept and compiles on its first run.
  saved <- file.path(where, "objects.rds")
  saveRDS(list(spec = spec, project = project), saved)
  for (restored in list(readRDS(saved), unserialize(serialize(list(spec = spec, project = project), NULL)))) {
    stopifnot(identical(plan_of(restored$spec$run()), "compiled"),
      identical(plan_of(restored$spec$run()), "reused"),
      nrow(restored$project$run()[["DM"]]$output) == 1L)
  }
  # The original keeps its own plan.
  stopifnot(identical(plan_of(spec$run()), "reused"))
  cat("restored objects start with nothing kept\n")
}
qualify_serialization()

unlink(root, recursive = TRUE)
