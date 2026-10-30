# Installed status and printing of every result and lazy object (D36, D37).
# Every printed line comes from the shared truth, which Python reads too: its
# lines were written from the forms of docs/design/interface-1.0/api.md at tag
# yamaa-v0.6.2 and the message templates of yaml/conditions.yaml before the
# engine ran. `{path}` stands for the path the test gave, and `{root}` for the
# study directory as an issue location names it. Rust renders the text:
# format() returns its lines, and print() writes them and returns the object
# invisibly.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped printing.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
define_fixtures <- normalizePath("define-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "printing")
statuses <- fixture_strings(read_host_fixture(host_fixture_root, "selftest")[["statuses"]])
created <- read_host_fixture(host_fixture_root, "public-define")[["created"]]

root <- tempfile("yamaa-printing-")
dir.create(root)
for (name in names(truth[["files"]])) {
  writeBin(charToRaw(enc2utf8(truth[["files"]][[name]])), file.path(root, name))
}

# `value` formats as `lines`, filling `{path}` and the study's `{root}`.
# print() writes each line ended by a line feed and returns `value` invisibly.
printed <- function(value, lines, path, label) {
  lines <- gsub("{path}", path, fixture_strings(lines), fixed = TRUE)
  lines <- gsub("{root}", normalizePath(root, winslash = "/", mustWork = TRUE),
    lines, fixed = TRUE)
  sunk <- rawConnection(raw(), "w")
  sink(sunk)
  shown <- withVisible(print(value))
  sink()
  bytes <- rawConnectionValue(sunk)
  close(sunk)
  expect_identical(format(value), lines, paste(label, "formats as its lines"))
  expect_identical(bytes, charToRaw(paste0(paste(lines, collapse = "\n"), "\n")), paste(label, "prints its lines"))
  stopifnot_case(label, "print() returns the object" = identical(shown$value, value),
    "print() returns it invisibly" = !shown$visible)
}

for (kind in c("spec", "project")) {
  lazy <- truth[["lazy"]][[kind]]
  make <- if (kind == "spec") yamaa_spec else yamaa_project
  path <- file.path(root, lazy[["path"]])
  printed(make(path), lazy[["lines"]], path, paste("a lazy", kind))
  # It reads nothing, so a file that does not exist prints too.
  absent <- file.path(root, "absent.yaml")
  printed(make(absent), lazy[["lines"]], absent, paste("a lazy", kind, "of an absent file"))
}
cat("a lazy object prints the path it was given\n")

result_of <- function(case, path) {
  switch(case[["call"]],
    check = yamaa_spec(path)$check(),
    project_check = yamaa_project(path)$check(),
    run = yamaa_spec(path)$run(),
    project_run = yamaa_project(path)$run(case[["section"]]))
}
for (case in truth[["cases"]]) {
  path <- file.path(root, case[["path"]])
  result <- result_of(case, path)
  label <- paste("a", case[["call"]], "of", case[["path"]])
  stopifnot_case(label, "the status is one of the vocabulary" =
      is.character(result$status) && length(result$status) == 1L && result$status %in% statuses,
    "the status is read-only" = inherits(tryCatch({ assign("status", "succeeded", envir = result); NULL },
      error = identity), "error"))
  expect_identical(result$status, case[["status"]], paste(label, "has the authored status"))
  printed(result, case[["lines"]], path, label)
}
cat("every result has its status and prints its summary\n")

for (case in truth[["cases"]]) {
  if (is.null(case[["members"]])) next
  runs <- yamaa_project(file.path(root, case[["path"]]))$run(case[["section"]])
  expect_identical(runs$names, vapply(case[["members"]], function(member) member[["name"]], character(1)),
    paste("the project run of", case[["path"]], "names its members"))
  for (member in case[["members"]]) {
    run <- runs[[member[["name"]]]]
    label <- paste("the member", member[["name"]], "of", case[["path"]])
    expect_identical(run$status, member[["status"]], paste(label, "has the authored status"))
    printed(run, member[["lines"]], root, label)
  }
}
cat("a project run member prints as a run under its name\n")

submission_truth <- truth[["submission"]]
fixture <- file.path(define_fixtures, submission_truth[["case"]])
directory <- file.path(root, "submission")
dir.create(directory)
stopifnot("the submission fixture is copied" =
  all(file.copy(file.path(fixture, c("project.yaml", "spec.yaml")), directory)) &&
    file.copy(file.path(fixture, "expected", "dm.csv"), directory))
path <- file.path(directory, "project.yaml")
submission <- yamaa_project(path)$submission(submission_truth[["section"]], created = created)
expect_identical(submission$status, submission_truth[["status"]], "the submission has the authored status")
printed(submission, submission_truth[["lines"]], path, "the submission")
cat("a submission has its status and prints its summary\n")

unlink(root, recursive = TRUE)
