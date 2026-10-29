# Installed yamaa_read_odm: the clinical-item table and its refusals. The
# reader's complete suite is native (odm_xml.rs); Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped read_odm.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
truth <- read_host_fixture(host_fixture_root, "read-odm")
issue_columns <- fixture_strings(read_host_fixture(host_fixture_root, "public-contract")[["issue_columns"]])
columns <- fixture_strings(truth[["columns"]])
integers <- fixture_strings(truth[["integer_columns"]])

work <- tempfile("yamaa-read-odm-"); dir.create(work)
archive <- truth[["archive"]]
members <- fixture_strings(archive[["members"]])
stopifnot("the archive's documents are copied" = all(file.copy(file.path(host_fixture_root, "odm", members), work)))
previous <- setwd(work)
utils::tar(archive[["name"]], files = members, tar = "internal")
setwd(previous)
# Sources the refusals read that are neither document nor archive.
for (name in names(truth[["files"]])) {
  writeBin(fixture_bytes(truth[["files"]][[name]]), file.path(work, name))
}
source_path <- function(name) {
  if (is.null(name)) file.path(work, "absent.xml") else file.path(work, name)
}

# Integer columns are decimal text in the truth and doubles in R; every other
# cell is character, NA where the truth is null.
expected_frame <- function(rows) {
  values <- lapply(columns, function(name) {
    cells <- lapply(rows, `[[`, name)
    if (name %in% integers) {
      vapply(cells, function(cell) if (is.null(cell)) NA_real_ else as.double(cell), double(1))
    } else {
      vapply(cells, function(cell) if (is.null(cell)) NA_character_ else cell, character(1))
    }
  })
  frame <- structure(values, names = columns, class = "data.frame")
  attr(frame, "row.names") <- c(NA_integer_, -length(rows))
  frame
}

for (read in truth[["reads"]]) {
  frame <- yamaa_read_odm(source_path(read[["document"]]))
  label <- paste("reading", read[["document"]])
  expect_identical(frame, expected_frame(read[["rows"]]), paste(label, "gives the shared truth"))
  expect_identical(unname(vapply(frame, typeof, "")), ifelse(columns %in% integers, "double", "character"),
    paste(label, "types its columns"))
}
cat("documents read into the fixed clinical item table\n")

selected <- Filter(function(read) identical(read[["document"]], archive[["selected"]]), truth[["reads"]])[[1L]]
expect_identical(yamaa_read_odm(file.path(work, archive[["name"]]), archive_member = archive[["selected"]]),
  expected_frame(selected[["rows"]]), "an archive member reads like its document")
cat("an archive member reads like its document\n")

for (refusal in truth[["refusals"]]) {
  options <- lapply(names(refusal[["options"]]), function(name) {
    value <- refusal[["options"]][[name]]
    if (identical(name, "archive_member")) value else as.numeric(value)
  })
  names(options) <- names(refusal[["options"]])
  source <- source_path(refusal[["document"]])
  error <- tryCatch(do.call(yamaa_read_odm, c(list(source), options)), yamaa_error = identity)
  # `{source}` stands for the source as given (#3030 D73, D158).
  given <- function(value) if (is.character(value)) gsub("{source}", source, value, fixed = TRUE) else value
  expected <- refusal[["issue"]]
  message <- given(expected[["message"]])
  # D39: the call names the source as given, then the issue.
  text <- paste0("cannot read ", source, ": ", message, " [", expected[["condition"]], ", ",
    expected[["rule"]], "]")
  label <- paste("the refusal", expected[["condition"]])
  stopifnot_case(label, "the refusal signals yamaa_error" = inherits(error, "yamaa_error"),
    "it carries an issues table" = is.data.frame(error$issues))
  expect_identical(conditionMessage(error), text, paste(label, "gives D39's text"))
  expect_identical(names(error$issues), issue_columns, paste(label, "has the issue columns"))
  row <- error$issues
  expect_identical(as.list(row[c("severity", "phase", "condition", "rule", "spec_paths", "message")]),
    list(severity = "error", phase = expected[["phase"]], condition = expected[["condition"]],
      rule = expected[["rule"]], spec_paths = list(as.character(unlist(expected[["spec_paths"]]))),
      message = message),
    paste(label, "is its one authored issue"))
  expect_context(jsonlite::fromJSON(row$context, simplifyVector = FALSE),
    lapply(expected[["context"]], given), paste(label, "has the authored context"))
}
cat("refusals signal yamaa_error carrying their issue\n")

limits <- truth$invalid_limits
for (argument in names(limits$arguments)) {
  for (case in limits$values) {
    source <- file.path(work, "unread.xml")
    value <- case$value
    shown <- if (is.character(value)) value else as.character(jsonlite::toJSON(value, auto_unbox = TRUE, null = "null"))
    fill <- function(text) gsub("{source}", source,
      gsub("{value}", shown, gsub("{argument}", argument, text, fixed = TRUE), fixed = TRUE), fixed = TRUE)
    expected <- limits$issue
    expected$message <- fill(expected$message)
    expected$spec_paths <- list(argument)
    expected$context <- as.character(jsonlite::toJSON(list(minimum = limits$arguments[[argument]], value = value),
      auto_unbox = TRUE, null = "null"))
    # R's NA variants and NULL all project the shared missing scalar.
    values <- if (is.null(value)) list(NULL, NA, NA_integer_, NA_real_) else list(value)
    for (given in values) {
      options <- setNames(list(given), argument)
      error <- tryCatch(do.call(yamaa_read_odm, c(list(source), options)), error = identity)
      label <- paste("invalid limit", argument, case$name)
      expect_identical(list(class = class(error), message = conditionMessage(error), call = conditionCall(error),
        columns = names(error$issues), issues = issue_columns_of(error$issues), paths = error$paths),
        list(class = c("yamaa_error", "error", "condition"), message = fill(limits$error), call = NULL,
          columns = issue_columns, issues = issue_truth(list(expected), work), paths = character()), label)
    }
  }
}
cat("invalid limits refuse at their argument before the source is read\n")
unlink(work, recursive = TRUE)
