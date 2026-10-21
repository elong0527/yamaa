# Installed yamaa_schema(): the JSON Schema of a file type as canonical JSON
# text, one character string (#3030 section 2.12, D149). The facade's own
# tests compare each schema with the file the repository commits in
# yaml/v1/json/; Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped schema.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "schema")

stopifnot(identical(names(formals(yamaa_schema)), "file_type"),
  identical(vapply(truth[["documents"]], function(document) document[["file_type"]], ""),
    fixture_strings(truth[["file_types"]])))
cat("yamaa_schema() takes one file type\n")

for (document in truth[["documents"]]) {
  text <- yamaa_schema(document[["file_type"]])
  stopifnot(is.character(text), length(text) == 1L, !is.na(text),
    !grepl("\n", text, fixed = TRUE), all(utf8ToInt(text) < 128L),
    startsWith(text, "{\"$defs\":{"))
  parsed <- jsonlite::fromJSON(text, simplifyVector = FALSE)
  # Canonical text keeps the members in key order.
  stopifnot(identical(names(parsed), fixture_strings(truth[["keys"]])),
    identical(parsed[["$schema"]], truth[["dialect"]]),
    identical(parsed[["title"]], document[["title"]]),
    identical(parsed[["$ref"]], document[["ref"]]),
    identical(yamaa_schema(document[["file_type"]]), text))
}
cat("each file type gives its JSON Schema as one canonical string\n")

for (refusal in truth[["refusals"]]) {
  error <- tryCatch(yamaa_schema(refusal[["file_type"]]), yamaa_error = identity)
  stopifnot(inherits(error, "yamaa_error"),
    identical(conditionMessage(error), refusal[["message"]]),
    identical(error$paths, character()), is.data.frame(error$issues),
    nrow(error$issues) == 1L,
    identical(error$issues[["condition"]], refusal[["condition"]]),
    identical(error$issues[["rule"]], refusal[["rule"]]),
    identical(error$issues[["message"]], refusal[["issue_message"]]),
    identical(error$issues[["context"]], refusal[["context"]]))
}
cat("any other text signals yamaa_error naming the condition\n")

for (file_type in list(NA_character_, c("spec", "project"), character(), 1, NULL)) {
  error <- tryCatch(yamaa_schema(file_type), error = identity)
  stopifnot(inherits(error, "error"), !inherits(error, "yamaa_error"))
}
cat("file_type must be one non-missing character string\n")
