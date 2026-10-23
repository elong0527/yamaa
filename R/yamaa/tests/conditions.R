# Installed yamaa_conditions(): the condition catalogue as a data frame
# (#3700). The facade's own tests check each entry against the registry;
# Python and the command line read the same truth: every condition in name
# order, and the authored rows whole, a list as a list-column cell holding a
# character vector and a missing cell as NA.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped conditions.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "conditions")

stopifnot("yamaa_conditions() takes no arguments" = length(formals(yamaa_conditions)) == 0L,
  "yamaa_conditions() is exported" = "yamaa_conditions" %in% getNamespaceExports("yamaa"))
cat("yamaa_conditions() takes no arguments and is exported\n")

catalogue <- yamaa_conditions()
columns <- fixture_strings(truth[["columns"]])
stopifnot("the catalogue is a data frame" = is.data.frame(catalogue))
expect_identical(names(catalogue), columns, "the catalogue's columns")
expect_identical(catalogue$condition, fixture_strings(truth[["names"]]), "every condition in name order")
cat("yamaa_conditions() names every condition in name order\n")

# A truth cell as the data frame holds it: a list as a character vector,
# empty for an empty list, and null text as NA.
list_cell <- function(cell) as.character(fixture_strings(cell))
text_cell <- function(cell) if (is.null(cell)) NA_character_ else cell
for (cells in truth[["rows"]]) {
  name <- cells[[1L]]
  at <- match(name, catalogue$condition)
  expected <- list(condition = name, severity = cells[[2L]], phases = list_cell(cells[[3L]]),
    rules = list_cell(cells[[4L]]), message = text_cell(cells[[5L]]),
    renamed_from = text_cell(cells[[6L]]), retired_in = text_cell(cells[[7L]]))
  actual <- lapply(stats::setNames(columns, columns), function(column) catalogue[[column]][[at]])
  expect_identical(actual, expected, paste("the row of", name))
}
cat("each authored row is the row of its condition\n")

expect_identical(yamaa_conditions(), catalogue, "a second call")
cat("a second call gives the same catalogue\n")
