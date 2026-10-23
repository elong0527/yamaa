# Project-run and project-function issue parity through the public R API.
# Each case of the shared project-parity truth writes its files, makes one
# call and compares every issue row it returns with the authored rows, all
# twelve columns of D28 (#3030): the issues of a project check, a listed
# specification's issues in a project run, the run of a specification the
# project names, or a submission. A case that makes several calls on its
# files lists each with its rows under calls. A case
# whose files or rows depend on the host language names them under
# host_files and under issues by language. {root} stands for the directory
# the case is written in, normalized with forward slashes, and {version} for
# the installed yamaa version. Python's installed_project_parity.py checks
# the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped project_parity.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "project-parity")
host <- "r"

# The issues the case's call returns.
issues_of <- function(call, root) {
  path <- file.path(root, call$path)
  switch(call$operation,
    project_check = yamaa_project(path)$check()$issues,
    project_run = yamaa_project(path)$run(call$section)[[call$member]]$issues,
    spec_run = yamaa_project(path)$spec(call$name)$run()$issues,
    submission = yamaa_project(path)$submission(call$section, created = call$created)$issues,
    stop("unknown operation: ", call$operation))
}

for (case in truth$cases) {
  root <- tempfile("yamaa-project-parity-"); dir.create(root)
  root <- normalizePath(root, winslash = "/")
  files <- case$files
  for (name in names(case$host_files[[host]])) files[[name]] <- case$host_files[[host]][[name]]
  for (name in names(files)) {
    target <- file.path(root, name)
    dir.create(dirname(target), recursive = TRUE, showWarnings = FALSE)
    writeBin(charToRaw(enc2utf8(files[[name]])), target)
  }
  # A case that makes several calls on its files lists each with its rows
  # under `calls`; any other makes its one `call`.
  calls <- if (is.null(case$calls)) list(list(call = case$call, issues = case$issues)) else case$calls
  # `{root}` and `{version}` stand for the case's directory and the installed
  # yamaa version; in the context they are JSON text.
  given <- c("{root}" = root, "{version}" = as.character(packageVersion("yamaa")))
  for (entry in calls) {
    expected <- if (is.null(names(entry$issues))) entry$issues else entry$issues[[host]]
    expected <- lapply(expected, function(issue) {
      for (field in c("message", "context")) if (!is.null(issue[[field]]))
        for (stand_in in names(given)) {
          value <- given[[stand_in]]
          if (field == "context") value <- gsub("\\", "\\\\", value, fixed = TRUE)
          issue[[field]] <- gsub(stand_in, value, issue[[field]], fixed = TRUE)
        }
      issue
    })
    # A truth row names its file relative to the case's directory; a location
    # names it by its absolute path (P-ISSUE-1).
    expect_identical(issue_columns_of(issues_of(entry$call, root)), issue_truth(expected, root),
      paste("the issue rows of the shared project parity case", case$name, "for its",
        entry$call$operation))
  }
  unlink(root, recursive = TRUE)
}
cat("project-run and project-function issue rows equal the shared truth\n")
