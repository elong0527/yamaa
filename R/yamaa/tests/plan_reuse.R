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
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
projects <- normalizePath("project-base", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
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
qualify_memory_file_fallback <- function() {
  memory <- read_host_fixture(host_fixture_root, "memory-inputs")
  case <- file.path(root, "memory-file-fallback")
  dir.create(case)
  for (name in names(memory$files)) write_text(file.path(case, name), memory$files[[name]])
  spec <- yamaa_spec(file.path(case, memory$entry))
  aliases <- unlist(memory$unknown$context$allowed, use.names = FALSE)
  expected_inputs <- setNames(rep(list(memory$file_details), length(aliases)), aliases)
  for (plan in c("compiled", "reused")) {
    run <- spec$run()
    run$save()
    inputs <- run$report
    inputs <- inputs[inputs$KIND == "input", , drop = FALSE]
    details <- setNames(lapply(inputs$DETAILS, jsonlite::fromJSON, simplifyVector = FALSE), inputs$NAME)
    details <- details[sort(names(details))]
    expect_identical(
      list(status = run$status, plan = plan_of(run), csv = read_text(file.path(case, "out.csv")), inputs = details),
      list(status = "succeeded", plan = plan, csv = memory$file_csv, inputs = expected_inputs),
      paste("alias override fixture retains file inputs with", plan, "plans")
    )
  }
  cat("alias override fixture preserves file fallback, report provenance and plan reuse\n")
}
qualify_memory_file_fallback()
refused <- function(code) inherits(tryCatch({ force(code); NULL }, yamaa_error = identity), "yamaa_error")
# Issue rows compare with the shared parity truth through issue_truth()
# (assert.R): a truth row names its file relative to the study's directory,
# `root`; a location names it by its absolute path (P-ISSUE-1).

# Run `spec` on one study's input and check its saved output or its issues.
spec_path <- file.path(root, "spec.yaml")
run_study <- function(spec, study) {
  write_text(file.path(root, "input.csv"), study$input_csv)
  output <- file.path(root, "output.csv")
  unlink(output)
  run <- spec$run()
  label <- paste("a run on the input", deparse(study$input_csv))
  if (length(study$issues) == 0L) {
    stopifnot_case(label, "the run has no issues" = nrow(run$issues) == 0L)
  } else {
    expect_identical(issue_columns_of(run$issues), issue_truth(study$issues, root),
      paste(label, "has the shared plan-reuse issue rows"))
  }
  if (is.null(study$output_csv)) {
    stopifnot_case(label, "a failed run has no output" = is.null(run$output),
      "its save is refused" = refused(run$save()), "nothing is written" = !file.exists(output))
  } else {
    run$save()
    expect_identical(read_text(output), study$output_csv, paste(label, "saves the authored CSV"))
  }
  run
}

qualify_reuse <- function() {
  write_text(spec_path, truth$specification_yaml)
  spec <- yamaa_spec(spec_path)
  compiles <- vapply(truth$sequence, function(name) compiled(run_study(spec, truth$studies[[name]])),
    logical(1))
  expect_identical(unname(compiles), c(TRUE, FALSE, FALSE), "A, B, A compile only on the first run")
  # Another object for the same file compiles on its own first run.
  stopifnot("another object compiles on its first run" = compiled(run_study(yamaa_spec(spec_path), truth$studies$A)))
  cat("later runs reuse the plan and read each input afresh\n")
}
qualify_reuse()

qualify_changed_specification <- function() {
  study <- truth$studies$A
  write_text(spec_path, truth$specification_yaml)
  spec <- yamaa_spec(spec_path)
  stopifnot("the first run compiles" = compiled(run_study(spec, study)))
  write_text(spec_path, truth$changed_specification_yaml)
  changed <- study
  changed$output_csv <- truth$changed_output_csv
  stopifnot("a changed specification compiles again" = compiled(run_study(spec, changed)),
    "the next run reuses the changed plan" = !compiled(run_study(spec, changed)))
  # A specification the engine refuses keeps nothing: the run reports the
  # refusal, and once it is fixed the next run compiles it.
  write_text(spec_path, 'yamaa: "1.0"\ntype: spec\ndomain: TEST\n')
  failed <- spec$run()
  stopifnot("a refused specification has no output" = is.null(failed$output),
    "a refused specification reports issues" = nrow(failed$issues) > 0L)
  write_text(spec_path, truth$specification_yaml)
  stopifnot("the fixed specification compiles" = compiled(run_study(spec, study)))
  cat("a changed specification compiles again and is never stale\n")
}
qualify_changed_specification()

qualify_changed_function_contract <- function() {
  stopifnot("the project case is copied" = file.copy(file.path(projects, "base-round"), root, recursive = TRUE))
  case <- file.path(root, "base-round")
  spec <- yamaa_project(file.path(case, "project.yaml"))$spec("TEST")
  expected <- read_text(file.path(case, "expected", "test.csv"))
  saved <- function(run) {
    run$save()
    read_text(file.path(case, "test.csv"))
  }
  first <- spec$run()
  stopifnot("the first run compiles" = compiled(first), "it saves the authored CSV" = identical(saved(first), expected),
    "the next run reuses the plan" = !compiled(spec$run()))
  # The function's default number of significant digits changes in its function
  # file: the run compiles the new contract and uses it.
  contract <- file.path(case, "functions", "rounded_value.yaml")
  original <- read_text(contract)
  write_text(contract, sub("default: 3", "default: 2",
    sub("args: {x: 2.375}, result: 2.38", "args: {x: 2.375}, result: 2.4",
      original, fixed = TRUE), fixed = TRUE))
  changed <- spec$run()
  stopifnot("a changed contract compiles again" = compiled(changed),
    "it rounds to the new default" = identical(saved(changed), "ID,RESULT\n01,2.4\n02,-1.1\n03,\n"),
    "the next run reuses the changed plan" = !compiled(spec$run()))
  write_text(contract, original)
  restored <- spec$run()
  stopifnot("the restored contract compiles again" = compiled(restored),
    "it saves the authored CSV" = identical(saved(restored), expected))
  # The project file changes: every plan it holds compiles again.
  project <- file.path(case, "project.yaml")
  write_text(project, paste0(read_text(project), "# reviewed\n"))
  stopifnot("a changed project file compiles again" = compiled(spec$run()),
    "the next run reuses the plan" = !compiled(spec$run()))
  cat("a changed function contract or project file compiles again\n")
}
qualify_changed_function_contract()

qualify_project <- function() {
  study <- file.path(root, "project-study")
  # A section lists only specifications of its model (SUB-3).
  dm <- paste0('yamaa: "1.0"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\n',
    "input: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\n",
    "columns:\n",
    "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n",
    "  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\n",
    "output: {path: ../out/dm.csv, columns: [USUBJID, AGE]}\n")
  adsl <- paste0('yamaa: "1.0"\ntype: spec\nmodel: ADaM\ndomain: ADSL\nkeys: [USUBJID]\n',
    "input: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\n",
    "columns:\n",
    "  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n",
    "  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\n",
    "output: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n")
  write_text(file.path(study, "raw", "dm.csv"), "USUBJID,AGE\nS1,30\nS2,41\n")
  write_text(file.path(study, "sdtm", "dm.yaml"), dm)
  write_text(file.path(study, "adam", "adsl.yaml"), adsl)
  write_text(file.path(study, "project.yaml"), paste0('yamaa: "1.0"\ntype: project\n',
    "sdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adsl.yaml]\n"))
  dir.create(file.path(study, "out"))
  project <- yamaa_project(file.path(study, "project.yaml"))
  saved <- function() {
    runs <- project$run()
    expect_identical(runs$names, c("DM", "ADSL"), "the project builds DM, then ADSL")
    stopifnot("the project run has no issues" = nrow(runs$issues) == 0L)
    runs$save()
    read_text(file.path(study, "out", "adsl.csv"))
  }
  expect_identical(saved(), "USUBJID,AGE\nS1,30\nS2,41\n", "the first project run")
  expect_identical(saved(), "USUBJID,AGE\nS1,30\nS2,41\n", "the second project run")
  # Each run reads the inputs afresh.
  write_text(file.path(study, "raw", "dm.csv"), "USUBJID,AGE\nS1,31\nS2,41\n")
  expect_identical(saved(), "USUBJID,AGE\nS1,31\nS2,41\n", "a project run reads a changed input")
  # A changed specification runs as changed.
  write_text(file.path(study, "adam", "adsl.yaml"),
    sub("[USUBJID, AGE]}", "[AGE, USUBJID]}", adsl, fixed = TRUE))
  expect_identical(saved(), "AGE,USUBJID\n31,S1\n41,S2\n", "a project run of a changed specification")
  expect_identical(saved(), "AGE,USUBJID\n31,S1\n41,S2\n", "the next run of the changed specification")
  # A spec object of the project keeps its own plan.
  spec <- project$spec("DM")
  expect_identical(c(compiled(spec$run()), compiled(spec$run())), c(TRUE, FALSE),
    "a project's spec object compiles once")
  cat("a project object reuses its plans and runs every change\n")
}
qualify_project()

plain <- paste0('yamaa: "1.0"\ntype: spec\ndomain: TEST\nkeys: [ID]\n',
  "input: {SRC: {path: input.csv, types: {ID: int}}}\n",
  "output: {path: out.csv, columns: [ID]}\n",
  "columns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n")
# Studies A and B with the same file names; B has one more row. A section
# lists only specifications of its model (SUB-3).
studies <- function(where) {
  for (name in c("a", "b")) {
    rows <- if (name == "a") "ID\n1\n2\n" else "ID\n1\n2\n3\n"
    write_text(file.path(where, name, "spec.yaml"), plain)
    write_text(file.path(where, name, "dm.yaml"),
      sub("out.csv", "dm.csv", sub("domain: TEST", "model: SDTM\ndomain: DM", plain, fixed = TRUE), fixed = TRUE))
    write_text(file.path(where, name, "project.yaml"), 'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [dm.yaml]\n')
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
  stopifnot("the first run in A compiles" = identical(plan_of(first), "compiled"),
    "it saves A" = identical(saved(first, "a", "out.csv"), a),
    "the next run in A reuses the plan" = identical(plan_of(spec$run()), "reused"),
    "the project run in A saves A" = identical(saved(project$run(), "a", "dm.csv"), a),
    "the member's first run compiles" = identical(plan_of(member$run()), "compiled"),
    "the member's next run reuses the plan" = identical(plan_of(member$run()), "reused"))
  # The same spellings now name study B: each object builds B and leaves A's
  # saved files as they were.
  setwd(file.path(where, "b"))
  moved <- spec$run()
  stopifnot("the first run in B compiles" = identical(plan_of(moved), "compiled"),
    "it saves B" = identical(saved(moved, "b", "out.csv"), b),
    "A's output stays" = identical(read_text(file.path(where, "a", "out.csv")), a),
    "B checks" = nrow(spec$check()$issues) == 0L,
    "the project run in B saves B" = identical(saved(project$run(), "b", "dm.csv"), b),
    "A's project output stays" = identical(read_text(file.path(where, "a", "dm.csv")), a))
  # `project$spec(name)` keeps its located specification, A's, with the
  # project's relative path, which now names B's project.
  stopifnot("the member compiles in B" = identical(plan_of(member$run()), "compiled"),
    "the member's next run in B reuses the plan" = identical(plan_of(member$run()), "reused"))
  # Back in A, the spec object compiles A again: it keeps one plan.
  setwd(file.path(where, "a"))
  stopifnot("back in A the spec object compiles again" = identical(plan_of(spec$run()), "compiled"))
  cat("relative paths after a working directory change build the new study\n")
}
qualify_working_directory()

qualify_link <- function() {
  if (.Platform$OS.type == "windows") return(invisible(NULL))
  where <- file.path(root, "linked")
  studies(where)
  link <- file.path(where, "study")
  stopifnot("the link names A" = file.symlink(file.path(where, "a"), link))
  spec <- yamaa_spec(file.path(link, "spec.yaml"))
  project <- yamaa_project(file.path(link, "project.yaml"))
  stopifnot("the first run compiles" = identical(plan_of(spec$run()), "compiled"),
    "the next run reuses the plan" = identical(plan_of(spec$run()), "reused"),
    "the project builds A's two rows" = nrow(project$run()[["DM"]]$output) == 2L)
  stopifnot("the link is retargeted to B" = file.remove(link) && file.symlink(file.path(where, "b"), link))
  moved <- spec$run()
  stopifnot("a run through the retargeted link compiles" = identical(plan_of(moved), "compiled"),
    "it builds B's three rows" = nrow(moved$output) == 3L,
    "the project builds B's three rows" = nrow(project$run()[["DM"]]$output) == 3L)
  cat("a path through a retargeted link builds the study it now names\n")
}
qualify_link()

qualify_serialization <- function() {
  where <- file.path(root, "serialized")
  write_text(file.path(where, "spec.yaml"), plain)
  write_text(file.path(where, "input.csv"), "ID\n1\n")
  write_text(file.path(where, "dm.yaml"),
    sub("out.csv", "dm.csv", sub("domain: TEST", "model: SDTM\ndomain: DM", plain, fixed = TRUE), fixed = TRUE))
  write_text(file.path(where, "project.yaml"), 'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [dm.yaml]\n')
  spec <- yamaa_spec(file.path(where, "spec.yaml"))
  project <- yamaa_project(file.path(where, "project.yaml"))
  stopifnot("the first run compiles" = identical(plan_of(spec$run()), "compiled"))
  invisible(project$run())
  # A restored object brings an expired holder, so it starts with nothing
  # kept and compiles on its first run.
  saved <- file.path(where, "objects.rds")
  saveRDS(list(spec = spec, project = project), saved)
  restorations <- list(readRDS = readRDS(saved),
    unserialize = unserialize(serialize(list(spec = spec, project = project), NULL)))
  for (how in names(restorations)) {
    restored <- restorations[[how]]
    stopifnot_case(paste("an object restored by", how),
      "its first run compiles" = identical(plan_of(restored$spec$run()), "compiled"),
      "its next run reuses the plan" = identical(plan_of(restored$spec$run()), "reused"),
      "its project builds the one row" = nrow(restored$project$run()[["DM"]]$output) == 1L)
  }
  # The original keeps its own plan.
  stopifnot("the original keeps its plan" = identical(plan_of(spec$run()), "reused"))
  cat("restored objects start with nothing kept\n")
}
qualify_serialization()

unlink(root, recursive = TRUE)
