# Installed compact flow verification parity and refusal controls (#2196).
library(yamaa)

check <- function(path) yamaa_spec(path)$check()
run <- function(path) yamaa_spec(path)$run()

run_compact_yaml <- function() {
  root <- tempfile("yamaa-compact-yaml-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  writeLines(c("ID", "a", "b", "c"), file.path(root, "input.csv"))
  spec <- file.path(root, "spec.yaml")
  base <- c(
    "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input:", "  SRC: input.csv", "columns:", "  - name: ID",
    "    type: str", "    label: ID", "    derivation: SRC.ID",
    "output:", "  path: out.csv", "  columns: [ID]"
  )
  write_spec <- function(verification, prior = FALSE) {
    prefix <- base
    if (prior) prefix <- c(base[1:3], "input: {SRC: input.csv}", base[6:length(base)])
    writeLines(c(prefix, verification), spec)
  }
  variants <- c(
    "verifications:\n  - row_count:\n      min: 3\n      max: 3",
    "verifications: [row_count: {min: 3, max: 3}]",
    "verifications: [{row_count: {min: 3, max: 3}}]",
    "verifications: [? row_count : {min: 3, max: 3}]",
    "verifications: [row_count: {min: 3, max: 3}, row_count: {min: 3, max: 3}]"
  )
  for (prior in c(FALSE, TRUE)) for (verification in variants) {
    write_spec(verification, prior)
    stopifnot(nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot(nrow(built$issues) == 0L, identical(built$output$ID, c("a", "b", "c")),
              is.null(built$save()))
    output <- file.path(root, "out.csv")
    stopifnot(identical(readBin(output, "raw", file.info(output)$size),
                        charToRaw("ID\na\nb\nc\n")))
  }
  for (bounds in c("min: 0, max: 2", "min: 4, max: 10")) {
    write_spec(paste0("verifications: [row_count: {", bounds, "}]"))
    stopifnot(nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot(is.null(built$output), identical(built$issues$condition, "row_count_failed"))
  }
  for (verification in c(
    "verifications: [row_count: {min: 3, max: 3]",
    "verifications: [row_count: {min: 3, max: 3}}]"
  )) {
    write_spec(verification)
    for (operation in list(check, run)) {
      stopifnot(identical(operation(spec)$issues$condition, "invalid_yaml"))
    }
  }
}

run_compact_yaml()
