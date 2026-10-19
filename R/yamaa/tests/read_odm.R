# Installed yamaa_read_odm: the clinical-item table and its refusals. The
# reader's complete suite is native (odm_xml.rs); Python reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped read_odm.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
truth <- read_host_fixture(host_fixture_root, "read-odm")
issue_columns <- fixture_strings(read_host_fixture(host_fixture_root, "public-contract")[["issue_columns"]])
columns <- fixture_strings(truth[["columns"]])
integers <- fixture_strings(truth[["integer_columns"]])

work <- tempfile("yamaa-read-odm-"); dir.create(work)
archive <- truth[["archive"]]
members <- fixture_strings(archive[["members"]])
stopifnot(all(file.copy(file.path(host_fixture_root, "odm", members), work)))
previous <- setwd(work)
utils::tar(archive[["name"]], files = members, tar = "internal")
setwd(previous)
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
  expected <- expected_frame(read[["rows"]])
  if (!identical(frame, expected)) {
    str(frame); str(expected)
    stop("read_odm differs from the shared truth: ", read[["document"]])
  }
  stopifnot(identical(unname(vapply(frame, typeof, "")),
    ifelse(columns %in% integers, "double", "character")))
}
cat("documents read into the fixed clinical item table\n")

selected <- Filter(function(read) identical(read[["document"]], archive[["selected"]]), truth[["reads"]])[[1L]]
stopifnot(identical(
  yamaa_read_odm(file.path(work, archive[["name"]]), archive_member = archive[["selected"]]),
  expected_frame(selected[["rows"]])))
cat("an archive member reads like its document\n")

for (refusal in truth[["refusals"]]) {
  options <- lapply(names(refusal[["options"]]), function(name) {
    value <- refusal[["options"]][[name]]
    if (identical(name, "archive_member")) value else as.numeric(value)
  })
  names(options) <- names(refusal[["options"]])
  error <- tryCatch(do.call(yamaa_read_odm, c(list(source_path(refusal[["document"]])), options)),
    yamaa_error = identity)
  prefix <- paste0("cannot read ODM: ODM read failure (", refusal[["code"]], "): ")
  # A reader failure is not a specification finding.
  stopifnot(inherits(error, "yamaa_error"), startsWith(conditionMessage(error), prefix),
    is.data.frame(error$issues), identical(names(error$issues), issue_columns),
    nrow(error$issues) == 0L)
}
cat("refusals signal yamaa_error naming the reader code\n")
unlink(work, recursive = TRUE)
