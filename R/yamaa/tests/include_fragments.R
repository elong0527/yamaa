# Specification fragments (LAYER-1 to LAYER-4) through the public R API. The
# shared include-fragments truth writes one study: two layers that a parent
# chain and an include list compose, and the files of four refusals. Each
# check case checks one entry and compares every issue row it returns with the
# authored rows, all twelve columns of D28 (#3030); {root} stands for the
# directory the study is written in, normalized with forward slashes. Each
# parent case checks a fragment with $check(parent = ) and the child that
# names that parent and includes it, and each refused parent raises its text
# and rows (#3694). The resolve case compares the layers the entry composes, in
# contribution order, the parent and include resources, the resolved columns,
# the origin of each named value, its layer and the line and column it was
# written at, and the fields the resolved specification never holds
# (LAYER-14). Python's installed_include_fragments.py
# checks the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped include_fragments.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "include-fragments")

root <- tempfile("yamaa-include-fragments-"); dir.create(root)
root <- normalizePath(root, winslash = "/")
for (name in names(truth$files)) {
  writeBin(charToRaw(enc2utf8(truth$files[[name]])), file.path(root, name))
}

for (case in truth$checks) {
  # `{root}` stands for the study's directory; in the context it is JSON text.
  expected <- lapply(case$issues, function(issue) {
    issue$message <- gsub("{root}", root, issue$message, fixed = TRUE)
    issue$context <- gsub("{root}", gsub("\\", "\\\\", root, fixed = TRUE), issue$context,
      fixed = TRUE)
    issue
  })
  # A truth row names its file relative to the study; a location names it by
  # its absolute path (P-ISSUE-1).
  expect_identical(issue_columns_of(yamaa_spec(file.path(root, case$path))$check()$issues),
    issue_truth(expected, root), paste("the issue rows of the shared check case", case$name))
}

# A fragment checked on a given parent returns the rows of the child that
# names that parent and includes the fragment (#3694).
for (case in truth$parent_checks) {
  expected <- issue_truth(case$issues, root)
  fragment <- yamaa_spec(file.path(root, case$fragment))$check(
    parent = file.path(root, case$parent))
  expect_identical(issue_columns_of(fragment$issues), expected,
    paste("the fragment checked on its parent in the shared parent case", case$name))
  expect_identical(issue_columns_of(yamaa_spec(file.path(root, case$child))$check()$issues),
    expected, paste("the child that includes the fragment in the shared parent case", case$name))
}

# A parent given for a file that is no fragment is refused with its error text
# and its rows, where {path} stands for the path the test gave and {parent} for
# the parent it gave; in a context, {parent} is JSON text.
for (case in truth$parent_refusals) {
  given <- file.path(root, case$path)
  parent <- file.path(root, case$parent)
  error <- tryCatch(yamaa_spec(given)$check(parent = parent), yamaa_error = identity)
  label <- paste("the shared parent refusal", case$name)
  stopifnot_case(label, "the refusal signals yamaa_error" = inherits(error, "yamaa_error"))
  text <- sub("{parent}", parent, sub("{path}", given, case$message, fixed = TRUE), fixed = TRUE)
  expect_identical(conditionMessage(error), text, paste(label, "gives D39's text"))
  expect_identical(error$paths, character(), paste(label, "writes nothing"))
  expected <- lapply(case$issues, function(issue) {
    issue$message <- gsub("{parent}", parent, issue$message, fixed = TRUE)
    issue$context <- gsub("{parent}", gsub("\\", "\\\\", parent, fixed = TRUE), issue$context,
      fixed = TRUE)
    issue
  })
  expect_identical(issue_columns_of(error$issues), issue_truth(expected, root),
    paste("the issue rows of", label))
}

resolved <- truth$resolve
entry <- yamaa_spec(file.path(root, resolved$path))$resolve()$entries[[1L]]
located <- function(name) file.path(root, name)
expect_identical(unlist(entry$layers), vapply(resolved$layers, located, ""),
  "the layers the entry composes, in contribution order")
layered <- Filter(function(resource)
  resource$role %in% c("parent_specification", "included_specification"), entry$resources)
expect_identical(
  lapply(layered, function(resource) list(spec_path = resource$spec_path, role = resource$role,
    declaring_source = resource$declaring_source, written = resource$written,
    location = resource$location$path)),
  lapply(resolved$resources, function(resource) {
    resource$declaring_source <- located(resource$declaring_source)
    resource$location <- located(resource$location)
    resource
  }),
  "the parent and include resources")
specification <- entry$specification
expect_identical(vapply(specification$columns, function(column) column$name, ""),
  unlist(resolved$columns), "the resolved columns in dependency order")
# An origin is compared whole, its fields by name; resolve() gives its line
# and column as doubles, as every JSON number.
by_name <- function(origin) origin[order(names(origin))]
expect_identical(
  lapply(names(resolved$origins), function(pointer) by_name(entry$origins[[pointer]])),
  lapply(unname(resolved$origins), function(origin) {
    origin$declaring_source <- located(origin$declaring_source)
    origin$line <- as.double(origin$line)
    origin$column <- as.double(origin$column)
    by_name(origin)
  }),
  "the origin of each named value: its layer, line and column")
for (field in unlist(resolved$absent_fields)) {
  stopifnot_case(paste("the resolved specification holds no", field),
    "the field is absent" = !(field %in% names(specification)))
}
unlink(root, recursive = TRUE)
cat("fragments compose after the parent chain, a fragment on a given parent checks as its child, and each refusal gives its shared rows\n")
