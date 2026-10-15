library(yamaa)

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
    stopifnot(nrow(yamaa_check(entry)$issues) == 0L)
    built <- yamaa_domain(entry)
    stopifnot(nrow(built$issues) == 0L, identical(dim(built$output), c(rows, 100L)),
              identical(names(built$output), names))
    for (column in seq_along(names)) {
      wanted <- if (column == 1L) seq_len(rows) - 1L else
        ((seq_len(rows) - 1L) + (column - 1L)) %% 10007L
      stopifnot(identical(as.character(built$output[[column]]), as.character(wanted)))
    }
    if (profile == "parquet") {
      consumer <- file.path(root, "consumer.yaml")
      writeLines(c(
        "schema_version: '1.0'", "domain: WIDE", "keys: [ID]", "input:",
        "  SRC: {path: output.parquet, schema: spec.yaml}", "columns:", columns,
        "output:", "  path: consumer.csv",
        paste0("  columns: [", paste(names, collapse = ", "), "]")
      ), consumer, useBytes = TRUE)
      produced <- yamaa_domain(consumer)
      stopifnot(nrow(produced$issues) == 0L,
                identical(dim(produced$output), c(rows, 100L)),
                identical(produced$save(), TRUE))
      output <- file.path(root, "consumer.csv")
      stopifnot(identical(readBin(output, "raw", n = file.info(output)$size), expected))
    }
    writeBin(charToRaw("changed\n"), file.path(root, "input.csv"))
    for (attempt in 1:2) {
      stopifnot(identical(built$save(), TRUE))
      output <- file.path(root, paste0("output.", profile))
      if (profile == "csv") {
        actual <- readBin(output, "raw", n = file.info(output)$size)
        stopifnot(identical(actual, expected))
      } else {
        stopifnot(file.exists(output), file.info(output)$size > 0L)
      }
    }
  }
}

run_longitudinal_capacity()
