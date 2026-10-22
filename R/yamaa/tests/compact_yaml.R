# Installed compact flow verification parity and refusal controls (#2196), and
# the raw NUL refusal (#3125).
library(yamaa)
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)

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
    label <- sprintf("%s after a %s input", deparse(verification), if (prior) "flow" else "block")
    stopifnot_case(label, "the variant checks" = nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot_case(label, "the variant runs" = nrow(built$issues) == 0L,
      "it keeps every row" = identical(built$output$ID, c("a", "b", "c")),
      "it saves" = returns_paths(built$save()),
      "it saves the same bytes" = identical(rawfile(file.path(root, "out.csv")), charToRaw("ID\na\nb\nc\n")))
  }
  for (bounds in c("min: 0, max: 2", "min: 4, max: 10")) {
    write_spec(paste0("verifications: [row_count: {", bounds, "}]"))
    stopifnot_case(bounds, "the bounds check" = nrow(check(spec)$issues) == 0L)
    built <- run(spec)
    stopifnot_case(bounds, "a failed count leaves no output" = is.null(built$output))
    expect_identical(built$issues$condition, "row_count_failed", paste("the bounds", bounds, "are executed"))
  }
  for (verification in c(
    "verifications: [row_count: {min: 3, max: 3]",
    "verifications: [row_count: {min: 3, max: 3}}]"
  )) {
    write_spec(verification)
    for (operation in c("check", "run")) {
      expect_identical(yamaa_spec(spec)[[operation]]()$issues$condition, "invalid_yaml",
        paste(operation, "of", deparse(verification)))
    }
  }
  # A raw NUL used to end the specification silently, dropping the verification
  # after it. R strings cannot hold NUL, so the bytes are written directly.
  writeBin(c(charToRaw(paste0(paste(base, collapse = "\n"), "\n")), as.raw(0L),
             charToRaw("verifications: [row_count: {min: 4}]\n")), spec)
  for (operation in c("check", "run")) {
    issues <- yamaa_spec(spec)[[operation]]()$issues
    expect_identical(as.list(issues[c("condition", "context")]), list(condition = "invalid_yaml",
      context = '{"column":1,"line":14,"reason":"found unprintable character"}'),
      paste(operation, "of a raw NUL"))
  }
}

run_compact_yaml()
