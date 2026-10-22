# Authored shared grammar acceptance/diagnostics through the installed public API.
library(yamaa)
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
# The grammar conditions are JSON truth, and jsonlite, which reads it, is a
# suggested package: where it is not installed, these checks are skipped and
# the CSV preservation checks below still run.
if (requireNamespace("jsonlite", quietly = TRUE)) {
  source(file.path(host_fixture_root,"reader.R"),local=TRUE)
  host_truth <- read_host_fixture(host_fixture_root,"public-grammar")
  source <- normalizePath("grammar-original", winslash = "/", mustWork = TRUE)
  root <- tempfile("yamaa-public-grammar-")
  dir.create(root)
  stopifnot("the grammar cases are copied" = file.copy(source, root, recursive = TRUE))
  root <- file.path(root, "grammar-original")
  vectors <- read.delim(file.path(root, "cases.tsv"), sep = "\t", quote = "", comment.char = "",
    stringsAsFactors = FALSE, check.names = FALSE, fileEncoding = "UTF-8", na.strings = character())
  grammar_conditions <- lapply(host_truth[["grammar_conditions"]],fixture_strings)
  for (i in seq_len(nrow(vectors))) {
    issues <- yamaa_spec(file.path(root, vectors$specification[i]))$run()$issues
    observed <- if (vectors$parse[i] == "reject") issues$condition else
      issues$condition[issues$condition %in% grammar_conditions[[vectors$contract[i]]]]
    expected <- if (vectors$parse[i] == "reject") vectors$public_condition[i] else character()
    label <- paste0(vectors$contract[i], "/", vectors$id[i])
    if (!identical(observed, expected)) fail_expectation(paste(label, "reports its authored condition"), issues$condition, expected)
    stopifnot_case(label, "nothing is written" = !file.exists(file.path(root, "unwritten.csv")))
  }
  unlink(dirname(root), recursive = TRUE)
  cat(nrow(vectors), "installed R public shared grammar checks passed\n")
} else {
  message("Skipped the shared grammar checks of public_grammar.R: their truth is JSON, and jsonlite, a suggested package, is not installed")
}

# Preserve the two CSV assertions that belonged to the legacy R evaluator:
# typeless leading-zero text and missing bare/quoted empty fields.
csv_preservation <- function() {
  root <- tempfile("yamaa-csv-preservation-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  writeBin(charToRaw('SUBJECT,SITE,BARE,QUOTED\n001,007,,""\n002,008,,""\n'),
    file.path(root, "input.csv"))
  spec <- paste0('yamaa: "1.0"\ntype: spec\ndomain: CSV\nkeys: [SUBJECT]\n',
    "input: {SRC: input.csv}\ncolumns:\n",
    "  - {name: SUBJECT, type: str, derivation: SRC.SUBJECT}\n",
    "  - {name: SITE, type: str, derivation: SRC.SITE}\n",
    "  - {name: BARE, type: str, derivation: SRC.BARE}\n",
    "  - {name: QUOTED, type: str, derivation: SRC.QUOTED}\n",
    "output: {path: output.csv, columns: [SUBJECT, SITE, BARE, QUOTED]}\n")
  writeBin(charToRaw(spec), file.path(root, "spec.yaml"))
  result <- yamaa_spec(file.path(root, "spec.yaml"))$run()
  stopifnot("the CSV run has no issues" = nrow(result$issues) == 0L)
  expect_identical(as.list(result$output), list(SUBJECT = c("001", "002"), SITE = c("007", "008"),
    BARE = c(NA_character_, NA_character_), QUOTED = c(NA_character_, NA_character_)),
    "leading zeros stay text and empty fields are missing")
  stopifnot("the CSV run saves" = returns_paths(result$save()))
  expect_identical(rawfile(file.path(root, "output.csv")),
    charToRaw("SUBJECT,SITE,BARE,QUOTED\n001,007,,\n002,008,,\n"), "the saved CSV")
}
csv_preservation()
cat("installed R public CSV leading-zero text and empty-field preservation passed\n")
