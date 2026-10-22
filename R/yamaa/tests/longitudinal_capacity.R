library(yamaa)
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

run_longitudinal_capacity <- function() {
  rows <- 50000L
  names <- c("ID", sprintf("C%03d", seq_len(99L)))
  header <- paste(names, collapse = ",")
  records <- vapply(seq_len(rows) - 1L, function(row) {
    paste(c(row, (row + seq_len(99L)) %% 10007L), collapse = ",")
  }, character(1))
  # Independently authored direct-projection truth, never candidate output.
  expected <- charToRaw(paste0(paste(c(header, records), collapse = "\n"), "\n"))
  root <- tempfile("longitudinal-capacity-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  columns <- vapply(names, function(name) {
    paste0("  - {name: ", name, ", type: int, label: ", name,
           ", derivation: SRC.", name, "}")
  }, character(1))
  entry <- file.path(root, "spec.yaml")
  for (profile in c("csv", "parquet")) {
    writeBin(expected, file.path(root, "input.csv"))
    writeLines(c(
      "schema_version: '1.0'", "domain: WIDE", "keys: [ID]", "input:",
      "  SRC:", "    path: input.csv", "    types:",
      paste0("      ", names, ": int"), "columns:", columns,
      "output:", paste0("  path: output.", profile),
      paste0("  columns: [", paste(names, collapse = ", "), "]")
    ), entry, useBytes = TRUE)
    label <- paste("the wide", profile, "run")
    stopifnot_case(label, "the specification checks" = nrow(check(entry)$issues) == 0L)
    built <- run(entry)
    stopifnot_case(label, "the run has no issues" = nrow(built$issues) == 0L)
    expect_identical(dim(built$output), c(rows, 100L), paste(label, "builds 50,000 rows of 100 columns"))
    expect_identical(names(built$output), names, paste(label, "names its columns"))
    for (column in seq_along(names)) {
      wanted <- if (column == 1L) seq_len(rows) - 1L else
        ((seq_len(rows) - 1L) + (column - 1L)) %% 10007L
      # Integer columns are plain doubles, exact for these magnitudes.
      stopifnot_case(paste(label, "column", names[[column]]),
        "the column holds the authored values" = identical(built$output[[column]], as.double(wanted)))
    }
    if (profile == "parquet") {
      # A link only types the stored artifact: save the producer before its
      # consumer reads it.
      stopifnot("the producer saves" = returns_paths(built$save()))
      consumer <- file.path(root, "consumer.yaml")
      writeLines(c(
        "schema_version: '1.0'", "domain: WIDE", "keys: [ID]", "input:",
        "  SRC: {path: output.parquet, schema: spec.yaml}", "columns:", columns,
        "output:", "  path: consumer.csv",
        paste0("  columns: [", paste(names, collapse = ", "), "]")
      ), consumer, useBytes = TRUE)
      consumed <- run(consumer)
      stopifnot("the consumer has no issues" = nrow(consumed$issues) == 0L)
      expect_identical(dim(consumed$output), c(rows, 100L), "the consumer reads every row and column")
      stopifnot("the consumer saves" = returns_paths(consumed$save()))
      expect_identical(rawfile(file.path(root, "consumer.csv")), expected, "the consumer saves the authored bytes")
    }
    writeBin(charToRaw("changed\n"), file.path(root, "input.csv"))
    for (attempt in 1:2) {
      where <- sprintf("%s, save %d after the input changed", label, attempt)
      stopifnot_case(where, "the held run saves" = returns_paths(built$save()))
      output <- file.path(root, paste0("output.", profile))
      if (profile == "csv") {
        expect_identical(rawfile(output), expected, paste(where, "writes the authored bytes"))
      } else {
        stopifnot_case(where, "the Parquet output is written" = file.exists(output) && file.info(output)$size > 0L)
      }
    }
  }
}

run_longitudinal_capacity()
