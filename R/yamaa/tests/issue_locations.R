# Issue locations (P-ISSUE-1, D28) through the public R API. Each case of the
# shared issue-locations truth writes its files, makes one call and compares
# its first issue's file, line and column with the authored location, one case
# for each rule of docs/design/interface-1.0/api.md at tag yamaa-v0.6.2,
# "Issue locations": a mapping member at its key, an expanded long form at the
# written node, a layered value in the layer whose value took effect, a
# refusal at the parent or include entry of a layer above the entry in that
# layer, a project path in the project file or in the file it lists, a data
# file's value at its declaration, a file the YAML reader refused at the
# position it reported, and an issue without a spec path without a location.
# Python's installed_issue_locations.py and the command line's parity test
# check the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped issue_locations.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "issue-locations")

# The issues the case's call returns, or the refusal carries.
issues_of <- function(call, root) {
  path <- file.path(root, call$path)
  switch(call$operation,
    check = yamaa_spec(path)$check()$issues,
    run = yamaa_spec(path)$run()$issues,
    project_check = yamaa_project(path)$check()$issues,
    submission = yamaa_project(path)$submission(call$section, created = call$created)$issues,
    select = tryCatch({
      yamaa_project(path)$spec(call$name)
      stop("selecting an unknown name refuses")
    }, yamaa_error = function(error) error$issues),
    stop("unknown operation: ", call$operation))
}

for (case in truth$cases) {
  root <- tempfile("yamaa-issue-locations-"); dir.create(root)
  # A location names the file by its absolute path as the engine resolves it,
  # with forward slashes.
  root <- normalizePath(root, winslash = "/")
  for (name in names(case$files)) {
    writeBin(charToRaw(enc2utf8(case$files[[name]])), file.path(root, name))
  }
  frame <- issues_of(case$call, root)
  expected <- case$issue
  located <- if (is.null(expected$file)) NA_character_ else file.path(root, expected$file)
  number <- function(value) if (is.null(value)) NA_real_ else as.double(value)
  actual <- list(frame$condition[[1L]], head(frame$spec_paths[[1L]], 1L),
    frame$file[[1L]], frame$line[[1L]], frame$column[[1L]])
  authored <- list(expected$condition,
    if (is.null(expected$spec_path)) character() else expected$spec_path,
    located, number(expected$line), number(expected$column))
  if (!identical(actual, authored)) {
    str(actual); str(authored)
    stop("the first issue is not located where the truth says: ", case$name)
  }
  unlink(root, recursive = TRUE)
}
cat("each first issue is located where the shared truth says\n")
