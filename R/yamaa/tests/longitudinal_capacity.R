# Installed public capacity acceptance for the approved longitudinal profile.
# The profile's size, its value rule, its source and output formats and how a
# run of it divides its input are the `longitudinal-capacity` family of the
# shared host truth, which the Python `installed_longitudinal_capacity` suite
# reads too.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped longitudinal_capacity.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "longitudinal-capacity")
# arrow, a suggested package, writes the Parquet source and reads the saved
# Parquet output back; without it the profile with a Parquet source is skipped
# and a saved Parquet output is only required to exist.
has_arrow <- requireNamespace("arrow", quietly = TRUE)
if (!has_arrow) message("Skipped the Parquet source of longitudinal_capacity.R: arrow, a suggested package, is not installed")
check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

run_longitudinal_capacity <- function() {
  rows <- truth$rows
  names <- c(truth$id_column, sprintf(truth$value_column_name, seq_len(truth$value_columns)))
  # The authored cells: the row index for the key, then the family's rule.
  # Integer columns are plain doubles, exact for these magnitudes (below 2^53).
  index <- as.double(seq_len(rows) - 1L)
  cells <- c(list(index), lapply(seq_len(truth$value_columns), function(column)
    truth$value_offset + (index + column) %% truth$value_modulus))
  names(cells) <- names
  # Independently authored direct-projection truth, never candidate output.
  records <- do.call(paste, c(lapply(cells, sprintf, fmt = "%.0f"), sep = ","))
  expected <- charToRaw(paste0(paste(c(paste(names, collapse = ","), records), collapse = "\n"), "\n"))
  rm(records)
  stopifnot("the source crosses the former 64 MiB descriptor capture ceiling" =
    length(expected) > truth$minimum_source_bytes)
  columns <- vapply(names, function(name) {
    paste0("  - {name: ", name, ", type: int, label: ", name,
           ", derivation: SRC.", name, "}")
  }, character(1))
  run_profile <- function(case) {
    label <- case$name
    root <- tempfile("longitudinal-capacity-")
    dir.create(root)
    on.exit(unlink(root, recursive = TRUE), add = TRUE)
    source_file <- file.path(root, paste0("input.", case$source))
    if (case$source == "csv") {
      writeBin(expected, source_file)
      input <- c("  SRC:", paste0("    path: ", basename(source_file)), "    types:",
                 paste0("      ", names, ": int"))
    } else {
      arrow::write_parquet(arrow::Table$create(as.data.frame(cells, optional = TRUE),
        schema = do.call(arrow::schema, setNames(rep(list(arrow::int64()), length(names)), names))),
        source_file)
      # An untyped Parquet source is written as its bare path.
      input <- paste0("  SRC: ", basename(source_file))
    }
    entry <- file.path(root, "spec.yaml")
    writeLines(c(
      'yamaa: "1.0"', "type: spec", "domain: WIDE", paste0("keys: [", truth$id_column, "]"), "input:",
      input, "columns:", columns,
      "output:", paste0("  path: output.", case$output),
      paste0("  columns: [", paste(names, collapse = ", "), "]")
    ), entry, useBytes = TRUE)
    stopifnot_case(label, "the specification checks" = nrow(check(entry)$issues) == 0L)
    built <- run(entry)
    stopifnot_case(label, "the run has no issues" = nrow(built$issues) == 0L)
    # How the run divided its input, which its measurements name (#3794): the
    # hosts' default partitions.
    observed <- jsonlite::fromJSON(
      yamaa:::.run_observations(built, include_measurements = TRUE), simplifyVector = FALSE)
    expect_identical(observed$measurements$partitions, truth$partitions,
      paste(label, "runs over the default partitions"))
    expect_identical(dim(built$output), c(rows, length(names)),
      paste(label, "builds every row and column"))
    expect_identical(names(built$output), names, paste(label, "names its columns"))
    for (name in names) {
      stopifnot_case(paste(label, "column", name),
        "the column holds the authored values" = identical(built$output[[name]], cells[[name]]))
    }
    output <- file.path(root, paste0("output.", case$output))
    if (case$output == "parquet") {
      # A link only types the stored artifact: save the producer before its
      # consumer reads it.
      stopifnot_case(label, "the producer saves" = returns_paths(built$save()))
      consumer <- file.path(root, "consumer.yaml")
      writeLines(c(
        'yamaa: "1.0"', "type: spec", "domain: WIDE", paste0("keys: [", truth$id_column, "]"), "input:",
        "  SRC: {path: output.parquet, schema: spec.yaml}", "columns:", columns,
        "output:", "  path: consumer.csv",
        paste0("  columns: [", paste(names, collapse = ", "), "]")
      ), consumer, useBytes = TRUE)
      consumed <- run(consumer)
      stopifnot_case(label, "the consumer has no issues" = nrow(consumed$issues) == 0L)
      expect_identical(dim(consumed$output), c(rows, length(names)),
        paste(label, "has a consumer that reads every row and column"))
      stopifnot_case(label, "the consumer saves" = returns_paths(consumed$save()))
      expect_identical(rawfile(file.path(root, "consumer.csv")), expected,
        paste(label, "has a consumer that saves the authored bytes"))
    }
    # Saving uses accepted bytes, including after source changes.
    writeBin(charToRaw("changed\n"), source_file)
    for (attempt in seq_len(truth$saves_after_change)) {
      where <- sprintf("%s, save %d after the input changed", label, attempt)
      stopifnot_case(where, "the held run saves" = returns_paths(built$save()))
      if (case$output == "csv") {
        expect_identical(rawfile(output), expected, paste(where, "writes the authored bytes"))
      } else {
        stopifnot_case(where, "the Parquet output is written" = file.exists(output) && file.info(output)$size > 0L)
        if (has_arrow) {
          saved <- arrow::read_parquet(output, as_data_frame = FALSE)
          expect_identical(list(names = names(saved), rows = saved$num_rows),
            list(names = names, rows = rows), paste(where, "saves every row and column"))
          for (name in names) {
            stopifnot_case(paste(where, "column", name),
              "the saved column holds the authored values" =
                identical(as.vector(saved[[name]]$cast(arrow::float64())), cells[[name]]))
          }
          rm(saved)
        }
      }
    }
  }
  for (case in truth$profiles) {
    if (case$source != "parquet" || has_arrow) run_profile(case)
  }
}

run_longitudinal_capacity()
