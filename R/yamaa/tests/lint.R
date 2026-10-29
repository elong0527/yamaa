# Installed yamaa_lint: the P-LINT-2 table, fixes, a partial restatement of an
# inherited mapping and values restated as they are inherited, which read the
# parent layer, and refusals. The lint's
# complete suite is native (the adapters' and the facade's lint tests); Python
# reads the same truth.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped lint.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
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
# column is the absolute path, with forward slashes, of the file named.
check_table <- function(frame, rows, label, file_name = truth[["file_name"]]) {
  stopifnot_case(label, "the findings are a data frame" = is.data.frame(frame))
  expect_identical(names(frame), columns, paste(label, "has the lint columns"))
  expect_identical(unname(vapply(frame, typeof, "")), ifelse(columns %in% integers, "double", "character"),
    paste(label, "types its columns"))
  expect_identical(nrow(frame), length(rows), paste(label, "has the authored rows"))
  files <- frame[["file"]]
  stopifnot_case(label, "a file has forward slashes" = !any(grepl("\\", files, fixed = TRUE)),
    "a file is the linted file" = all(endsWith(files, paste0("/", file_name))),
    "a file is absolute" = all(grepl("^(/|[A-Za-z]:/)", files)))
  for (name in setdiff(columns, "file")) {
    cells <- vapply(rows, function(row) row[[name]], character(1))
    expected <- if (name %in% integers) as.double(cells) else cells
    expect_identical(frame[[name]], expected, paste(label, "column", name, "is the shared truth"))
  }
}

check_table(yamaa_lint(path), truth[["rows"]], "lint")
check_table(yamaa_lint(c(path), fix = FALSE), truth[["rows"]], "lint without fix")
expect_identical(read_text(path), truth[["specification_yaml"]], "lint without fix writes nothing")
cat("findings come as the six-column table\n")

check_table(yamaa_lint(path, fix = TRUE), truth[["fixed_rows"]], "lint with fix")
expect_identical(read_text(path), truth[["fixed_yaml"]], "fix writes the proved fixes")
check_table(yamaa_lint(path), truth[["fixed_rows"]], "lint after fix")
cat("fix rewrites the file and lists what remains\n")

# A layer that restates its parent's row template without one derivation, and
# one that writes values as it inherits them: the findings read the parent,
# and fix leaves the file as written.
for (key in c("partial_restatement", "restated_inherited_value")) {
  restatement <- truth[[key]]
  for (name in names(restatement[["files"]])) write_text(file.path(work, name), restatement[["files"]][[name]])
  child <- file.path(work, restatement[["file_name"]])
  for (fix in c(FALSE, TRUE)) {
    label <- paste(key, "fix =", fix)
    check_table(yamaa_lint(child, fix = fix), restatement[["rows"]], label, restatement[["file_name"]])
    expect_identical(read_text(child), restatement[["files"]][[restatement[["file_name"]]]],
      paste(label, "leaves the file as written"))
  }
}
cat("a layer is compared with its parent and fix leaves it\n")

write_text(path, truth[["specification_yaml"]])
for (refusal in truth[["refusals"]]) {
  refused <- file.path(work, "refused.yaml")
  if (!is.null(refusal[["yaml"]])) write_text(refused, refusal[["yaml"]])
  error <- tryCatch(yamaa_lint(c(path, refused), fix = TRUE), yamaa_error = identity)
  stopifnot_case(refusal[["condition"]], "the refusal signals yamaa_error" = inherits(error, "yamaa_error"),
    "it carries an issues table" = is.data.frame(error$issues))
  expect_identical(error$issues[["condition"]], refusal[["condition"]], "the refusal names its condition")
  # A refused call writes nothing.
  expect_identical(read_text(path), truth[["specification_yaml"]], paste(refusal[["condition"]], "writes nothing"))
  unlink(refused)
}
cat("refusals signal yamaa_error naming the condition\n")

for (paths in list(character(), NA_character_, 1)) {
  error <- tryCatch(yamaa_lint(paths), error = identity)
  stopifnot_case(paste("paths", deparse(paths)), "the mistake is a package error" =
    inherits(error, "error") && inherits(error, "yamaa_error"))
}
cat("paths must be a non-empty character vector\n")
unlink(work, recursive = TRUE)
