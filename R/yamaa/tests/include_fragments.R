# Specification fragments (LAYER-1 to LAYER-4) through the public R API. The
# shared include-fragments truth writes one study: two layers that a parent
# chain and an include list compose, and the files of four refusals. Each
# check case checks one entry and compares every issue row it returns with the
# authored rows, all twelve columns of D28 (#3030); {root} stands for the
# directory the study is written in, normalized with forward slashes. The
# resolve case compares the layers the entry composes, in contribution order,
# the parent and include resources, the resolved columns, the layer each named
# value comes from, and the fields the resolved specification never holds
# (LAYER-14). Python's installed_include_fragments.py checks the same truth.
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
expect_identical(
  lapply(names(resolved$declaring_sources), function(pointer)
    entry$origins[[pointer]]$declaring_source),
  lapply(unname(resolved$declaring_sources), located),
  "the layer each named value comes from")
for (field in unlist(resolved$absent_fields)) {
  stopifnot_case(paste("the resolved specification holds no", field),
    "the field is absent" = !(field %in% names(specification)))
}
unlink(root, recursive = TRUE)
cat("fragments compose after the parent chain, and each refusal gives its shared rows\n")
