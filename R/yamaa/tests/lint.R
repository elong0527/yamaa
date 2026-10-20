# Installed yamaa_lint: the P-LINT-2 table, fixes and refusals. The lint's
# complete suite is native (the adapters' lint tests); Python reads the same
# truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped lint.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- if (file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "lint")
columns <- fixture_strings(truth[["columns"]])
integers <- fixture_strings(truth[["integer_columns"]])

work <- tempfile("yamaa-lint-")
dir.create(work)
path <- file.path(work, truth[["file_name"]])
write_text <- function(path, text) writeBin(charToRaw(enc2utf8(text)), path)
read_text <- function(path) rawToChar(readBin(path, "raw", file.info(path)$size))
write_text(path, truth[["specification_yaml"]])

# Integer columns are decimal text in the truth and doubles in R; the file
# column is the absolute path with forward slashes.
check_table <- function(frame, rows) {
  stopifnot(is.data.frame(frame), identical(names(frame), columns))
  stopifnot(identical(unname(vapply(frame, typeof, "")),
    ifelse(columns %in% integers, "double", "character")))
  stopifnot(nrow(frame) == length(rows))
  files <- frame[["file"]]
  stopifnot(!grepl("\\", files, fixed = TRUE),
    endsWith(files, paste0("/", truth[["file_name"]])),
    grepl("^(/|[A-Za-z]:/)", files))
  for (name in setdiff(columns, "file")) {
    cells <- vapply(rows, function(row) row[[name]], character(1))
    expected <- if (name %in% integers) as.double(cells) else cells
    if (!identical(frame[[name]], expected)) {
      str(frame[[name]]); str(expected)
      stop("lint column differs from the shared truth: ", name)
    }
  }
}

check_table(yamaa_lint(path), truth[["rows"]])
check_table(yamaa_lint(c(path), fix = FALSE), truth[["rows"]])
stopifnot(identical(read_text(path), truth[["specification_yaml"]]))
cat("findings come as the six-column table\n")

check_table(yamaa_lint(path, fix = TRUE), truth[["fixed_rows"]])
stopifnot(identical(read_text(path), truth[["fixed_yaml"]]))
check_table(yamaa_lint(path), truth[["fixed_rows"]])
cat("fix rewrites the file and lists what remains\n")

write_text(path, truth[["specification_yaml"]])
for (refusal in truth[["refusals"]]) {
  refused <- file.path(work, "refused.yaml")
  if (!is.null(refusal[["yaml"]])) write_text(refused, refusal[["yaml"]])
  error <- tryCatch(yamaa_lint(c(path, refused), fix = TRUE), yamaa_error = identity)
  stopifnot(inherits(error, "yamaa_error"), is.data.frame(error$issues),
    identical(error$issues[["condition"]], refusal[["condition"]]))
  # A refused call writes nothing.
  stopifnot(identical(read_text(path), truth[["specification_yaml"]]))
  unlink(refused)
}
cat("refusals signal yamaa_error naming the condition\n")

for (paths in list(character(), NA_character_, 1)) {
  error <- tryCatch(yamaa_lint(paths), error = identity)
  stopifnot(inherits(error, "error"), !inherits(error, "yamaa_error"))
}
cat("paths must be a non-empty character vector\n")
unlink(work, recursive = TRUE)
