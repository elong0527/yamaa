# Installed yamaa_schema(): the JSON Schema of a file type as canonical JSON
# text, one character string (#3030 section 2.12, D149). The facade's own
# tests compare each schema with the file the repository commits in
# yaml/json/; Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped schema.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "schema")

expect_identical(names(formals(yamaa_schema)), "file_type", "yamaa_schema()'s arguments")
expect_identical(vapply(truth[["documents"]], function(document) document[["file_type"]], ""),
  fixture_strings(truth[["file_types"]]), "the truth has a document for each file type")
cat("yamaa_schema() takes one file type\n")

for (document in truth[["documents"]]) {
  text <- yamaa_schema(document[["file_type"]])
  label <- paste("the", document[["file_type"]], "schema")
  stopifnot_case(label, "the schema is one string" = is.character(text) && length(text) == 1L && !is.na(text),
    "it is one line" = !grepl("\n", text, fixed = TRUE), "it is ASCII" = all(utf8ToInt(text) < 128L),
    "it starts with its definitions" = startsWith(text, "{\"$defs\":{"))
  parsed <- jsonlite::fromJSON(text, simplifyVector = FALSE)
  # Canonical text keeps the members in key order.
  expect_identical(names(parsed), fixture_strings(truth[["keys"]]), paste(label, "keeps its keys in order"))
  expect_identical(parsed[c("$schema", "title", "$ref")],
    list("$schema" = truth[["dialect"]], title = document[["title"]], "$ref" = document[["ref"]]),
    paste(label, "states its dialect, title and reference"))
  expect_identical(yamaa_schema(document[["file_type"]]), text, paste(label, "is the same text twice"))
}
cat("each file type gives its JSON Schema as one canonical string\n")

for (refusal in truth[["refusals"]]) {
  error <- tryCatch(yamaa_schema(refusal[["file_type"]]), yamaa_error = identity)
  label <- paste("the file type", deparse(refusal[["file_type"]]))
  stopifnot_case(label, "the refusal signals yamaa_error" = inherits(error, "yamaa_error"),
    "it carries an issues table" = is.data.frame(error$issues))
  expect_identical(conditionMessage(error), refusal[["message"]], paste(label, "gives D39's text"))
  expect_identical(error$paths, character(), paste(label, "writes nothing"))
  expect_identical(as.list(error$issues[c("condition", "rule", "message", "context")]),
    list(condition = refusal[["condition"]], rule = refusal[["rule"]], message = refusal[["issue_message"]],
      context = refusal[["context"]]),
    paste(label, "is its one authored issue"))
}
cat("any other text signals yamaa_error naming the condition\n")

for (file_type in list(NA_character_, c("spec", "project"), character(), 1, NULL)) {
  error <- tryCatch(yamaa_schema(file_type), error = identity)
  stopifnot_case(paste("the argument", deparse(file_type)), "the mistake is a package error" =
    inherits(error, "error") && inherits(error, "yamaa_error"))
}
cat("file_type must be one non-missing character string\n")
