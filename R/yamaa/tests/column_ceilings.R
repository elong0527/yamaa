# Boundary controls for the 1,024-column ceilings (#2185, #2288 item 2).
# The shared compiler admits 1,024 declared columns, output columns and
# typed source fields, and refuses 1,025 with a compiler-limit rejection.
library(yamaa)

write_case <- function(root, columns, typed_fields = 0L) {
  dir.create(root, recursive = TRUE, showWarnings = FALSE)
  names <- c("ID", sprintf("C%04d", seq_len(columns - 1L)))
  fields <- sprintf("F%04d", seq_len(typed_fields))
  header <- paste(c("ID", fields), collapse = ",")
  row <- paste(c("S-001", rep("x", typed_fields)), collapse = ",")
  writeLines(c(header, row), file.path(root, "input.csv"))
  source <- "input.csv"
  if (typed_fields > 0L) {
    types <- paste(sprintf("%s: str", fields), collapse = ", ")
    source <- sprintf("{path: input.csv, types: {%s}}", types)
  }
  column_lines <- c(
    "  - {name: ID, type: str, derivation: SRC.ID}",
    sprintf("  - {name: %s, type: str, derivation: {literal: x}}", names[-1L])
  )
  spec <- c(
    "schema_version: '1.0'",
    "domain: TEST",
    "keys: [ID]",
    sprintf("input: {SRC: %s}", source),
    "columns:",
    column_lines,
    sprintf("output: {path: out.csv, columns: [%s]}", paste(names, collapse = ", "))
  )
  path <- file.path(root, "spec.yaml")
  writeLines(spec, path)
  path
}

compiler_limit <- function(result) {
  issues <- result$issues
  nrow(issues) > 0L &&
    any(issues$condition == "engine_rejected" &
      grepl("compiler_limit", issues$context, fixed = TRUE))
}

root <- tempfile("yamaa-column-ceilings-")
on.exit(unlink(root, recursive = TRUE))

spec <- write_case(file.path(root, "admit-columns"), 1024L)
stopifnot(nrow(yamaa_check(spec)$issues) == 0L)
built <- yamaa_domain(spec)
stopifnot(nrow(built$issues) == 0L, ncol(built$output) == 1024L)

spec <- write_case(file.path(root, "refuse-columns"), 1025L)
stopifnot(compiler_limit(yamaa_check(spec)))

spec <- write_case(file.path(root, "admit-types"), 1L, 1024L)
stopifnot(nrow(yamaa_check(spec)$issues) == 0L)

spec <- write_case(file.path(root, "refuse-types"), 1L, 1025L)
stopifnot(compiler_limit(yamaa_check(spec)))
