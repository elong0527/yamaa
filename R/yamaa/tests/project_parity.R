# Project-run and project-function issue parity through the public R API.
# Each case of the shared project-parity truth writes its files, makes one
# call and compares every issue row it returns with the authored rows, all
# twelve columns of D28 (#3030): a listed specification's issues in a project
# run, the run of a specification the project names, or a submission. A case
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
truth <- read_host_fixture(host_fixture_root, "project-parity")
host <- "r"

# The issues the case's call returns.
issues_of <- function(call, root) {
  path <- file.path(root, call$path)
  switch(call$operation,
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
  frame <- issues_of(case$call, root)
  expected <- if (is.null(names(case$issues))) case$issues else case$issues[[host]]
  # `{root}` and `{version}` stand for the case's directory and the installed
  # yamaa version; in the context they are JSON text.
  values <- c("{root}" = root, "{version}" = as.character(packageVersion("yamaa")))
  filled <- function(text, json) {
    for (stand_in in names(values)) {
      value <- values[[stand_in]]
      if (json) value <- gsub("\\", "\\\\", value, fixed = TRUE)
      text <- gsub(stand_in, value, text, fixed = TRUE)
    }
    text
  }
  text <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_character_ else issue[[field]], "")
  number <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_real_ else as.double(issue[[field]]), 0)
  actual <- list(severity = frame$severity, phase = frame$phase, condition = frame$condition,
    rule = frame$rule, spec_paths = frame$spec_paths, file = frame$file, line = frame$line,
    column = frame$column, message = frame$message, context = frame$context,
    source_span = frame$source_span, operand_route = frame$operand_route)
  authored <- list(severity = text("severity"), phase = text("phase"),
    condition = text("condition"), rule = text("rule"),
    spec_paths = lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
    # A truth row names its file relative to the case's directory; a location
    # names it by its absolute path (P-ISSUE-1).
    file = vapply(expected, function(issue) if (is.null(issue$file)) NA_character_ else
      normalizePath(file.path(root, issue$file), winslash = "/", mustWork = TRUE), ""),
    line = number("line"), column = number("column"),
    message = enc2utf8(vapply(text("message"), filled, "", json = FALSE, USE.NAMES = FALSE)),
    context = enc2utf8(vapply(text("context"), filled, "", json = TRUE, USE.NAMES = FALSE)),
    source_span = lapply(expected, function(issue)
      if (is.null(issue$source_span)) NULL else as.double(unlist(issue$source_span))),
    operand_route = lapply(expected, function(issue)
      if (is.null(issue$operand_route)) NULL else as.character(unlist(issue$operand_route))))
  if (!identical(actual, authored)) {
    str(actual); str(authored)
    stop("issue rows differ from the shared project parity truth: ", case$name)
  }
  unlink(root, recursive = TRUE)
}
cat("project-run and project-function issue rows equal the shared truth\n")
