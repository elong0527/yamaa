# Authored shared grammar acceptance/diagnostics through the installed public API.
library(yamaa)
source <- system.file("grammar-original", package = "yamaa", mustWork = TRUE)
root <- tempfile("yamaa-public-grammar-")
dir.create(root)
stopifnot(file.copy(source, root, recursive = TRUE))
root <- file.path(root, "grammar-original")
vectors <- read.delim(file.path(root, "cases.tsv"), sep = "\t", quote = "", comment.char = "",
  stringsAsFactors = FALSE, check.names = FALSE, fileEncoding = "UTF-8", na.strings = character())
grammar_conditions <- list(
  numeric = c("invalid_numeric_expression", "prohibited_function", "prohibited_construct"),
  aggregate = c("invalid_aggregate_expression", "nested_reduction", "prohibited_function", "prohibited_construct"),
  predicate = c("invalid_predicate", "integer_overflow"),
  `string-template` = "invalid_string_template")
for (i in seq_len(nrow(vectors))) {
  issues <- yamaa_domain(file.path(root, vectors$specification[i]))$issues
  observed <- if (vectors$parse[i] == "reject") issues$condition else
    issues$condition[issues$condition %in% grammar_conditions[[vectors$contract[i]]]]
  expected <- if (vectors$parse[i] == "reject") vectors$public_condition[i] else character()
  if (!identical(observed, expected)) stop(vectors$contract[i], "/", vectors$id[i],
    ": expected ", paste(expected, collapse = ","), "; observed ", paste(issues$condition, collapse = ","))
  stopifnot(!file.exists(file.path(root, "unwritten.csv")))
}
unlink(dirname(root), recursive = TRUE)
cat(nrow(vectors), "installed R public shared grammar checks passed\n")

# Preserve the two CSV assertions that belonged to the legacy R evaluator:
# typeless leading-zero text and missing bare/quoted empty fields.
csv_preservation <- function() {
  root <- tempfile("yamaa-csv-preservation-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  writeBin(charToRaw('SUBJECT,SITE,BARE,QUOTED\n001,007,,""\n002,008,,""\n'),
    file.path(root, "input.csv"))
  spec <- paste0("schema_version: '1.0'\ndomain: CSV\nkeys: [SUBJECT]\n",
    "input: {SRC: input.csv}\ncolumns:\n",
    "  - {name: SUBJECT, type: str, derivation: SRC.SUBJECT}\n",
    "  - {name: SITE, type: str, derivation: SRC.SITE}\n",
    "  - {name: BARE, type: str, derivation: SRC.BARE}\n",
    "  - {name: QUOTED, type: str, derivation: SRC.QUOTED}\n",
    "output: {path: output.csv, columns: [SUBJECT, SITE, BARE, QUOTED]}\n")
  writeBin(charToRaw(spec), file.path(root, "spec.yaml"))
  result <- yamaa_domain(file.path(root, "spec.yaml"))
  stopifnot(nrow(result$issues) == 0L,
    identical(result$output$SUBJECT, c("001", "002")),
    identical(result$output$SITE, c("007", "008")),
    identical(result$output$BARE, c(NA_character_, NA_character_)),
    identical(result$output$QUOTED, c(NA_character_, NA_character_)),
    identical(result$save(), TRUE))
  path <- file.path(root, "output.csv")
  stopifnot(identical(readBin(path, "raw", n = file.info(path)$size),
    charToRaw("SUBJECT,SITE,BARE,QUOTED\n001,007,,\n002,008,,\n")))
}
csv_preservation()
cat("installed R public CSV leading-zero text and empty-field preservation passed\n")
