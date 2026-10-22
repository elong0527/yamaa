# Shared assertions and helpers of the installed R suites (R/yamaa/tests).
# A suite sources this file from its staged host-fixtures directory:
#   source(file.path(host_fixture_root, "assert.R"), local = TRUE)
# Sourcing it needs base R alone. The helpers that decode JSON call jsonlite,
# a suggested package, only when they run, and those that compare contexts
# call context_equal() from context.R, so a suite that uses them sources
# context.R too and skips first when jsonlite is not installed.

# A value as the text a failure prints: its deparsed form, cut to `limit`
# characters so that a large table or report still gives a readable message.
shown_value <- function(value, limit = 3000L) {
  text <- paste(deparse(value, width.cutoff = 100L, nlines = 200L), collapse = "\n")
  size <- nchar(text, type = "chars", allowNA = TRUE)
  if (!is.na(size) && size > limit)
    text <- sprintf("%s\n... (%d more characters)", substr(text, 1L, limit), size - limit)
  text
}
# Fail with `label`, the case and what it expects, and both values.
fail_expectation <- function(label, actual, expected) {
  stop(sprintf("%s\nactual:\n%s\nexpected:\n%s", label, shown_value(actual), shown_value(expected)),
    call. = FALSE)
}
# `actual` is identical to `expected`; otherwise the failure names `label`
# and shows both values. Returns `actual`.
expect_identical <- function(actual, expected, label) {
  if (!identical(actual, expected)) fail_expectation(label, actual, expected)
  invisible(actual)
}
# stopifnot() for one case of a loop: each clause is named for what it
# expects, and a failure names `case` first.
stopifnot_case <- function(case, ...) {
  tryCatch(stopifnot(...), error = function(error)
    stop(case, ": ", conditionMessage(error), call. = FALSE))
}
# Two decoded JSON contexts are equal with object key order ignored at every
# level (context.R); otherwise the failure names `label` and shows both.
expect_context <- function(actual, expected, label) {
  if (!context_equal(actual, expected)) fail_expectation(label, actual, expected)
  invisible(actual)
}

# save() returns the absolute paths it wrote, invisibly, with forward slashes,
# in publication order (D38); each one exists once written.
returns_paths <- function(paths) is.character(paths) && length(paths) > 0L && !anyNA(paths) &&
  all(grepl("^(/|[[:alpha:]]:/)", paths)) && !any(grepl("\\", paths, fixed = TRUE)) &&
  all(file.exists(paths))
# The exact bytes of a file.
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)

# An issue's file as its printed location names it (rule 3 of P-PRINT-1):
# relative to the working directory when the file is beneath it, and absolute
# otherwise. Beneath is a matter of directories, not spellings: the engine
# names the file canonically, in its own spelling, while the working directory
# or normalizePath() may spell a directory another way, such as with Windows
# 8.3 short names. So each directory above the file is compared with the
# working directory as normalizePath() spells both, for both callers.
printed_file <- function(file) {
  here <- normalizePath(getwd(), winslash = "/")
  parts <- strsplit(file, "/", fixed = TRUE)[[1L]]
  for (depth in rev(seq_len(length(parts) - 1L))) {
    # The first part is the root: "" for "/", or a drive such as "C:".
    directory <- paste(parts[seq_len(depth)], collapse = "/")
    if (depth == 1L) directory <- paste0(directory, "/")
    if (identical(normalizePath(directory, winslash = "/", mustWork = FALSE), here))
      return(paste(parts[-seq_len(depth)], collapse = "/"))
  }
  file
}

# D39's line for the first of `issues`: `<message> [<condition>, <rule>]`,
# without the rule when it has none, then the count of the other issues.
issue_line <- function(issues) {
  rule <- issues$rule[[1L]]
  line <- sprintf("%s [%s]", issues$message[[1L]],
    if (is.na(rule)) issues$condition[[1L]] else paste(issues$condition[[1L]], rule, sep = ", "))
  others <- nrow(issues) - 1L
  if (others > 0L) line <- sprintf("%s (and %d more %s)", line, others, if (others == 1L) "issue" else "issues")
  line
}
# Whether `failure` has D39's text for a refused save of `issues`:
# `cannot save <subject>: ` and then their issue line.
save_refusal_text <- function(failure, issues) {
  text <- conditionMessage(failure)
  startsWith(text, "cannot save ") && endsWith(text, paste0(": ", issue_line(issues)))
}
# Whether saving `result` signals yamaa_error carrying the result's own issues,
# with D39's text.
save_refused <- function(result) {
  failure <- tryCatch({ result$save(); NULL }, error = identity)
  inherits(failure, "yamaa_error") && identical(failure$issues, result$issues) &&
    save_refusal_text(failure, failure$issues)
}

# Issue rows as the shared parity truth holds them, all twelve columns of D28:
# context as canonical JSON text, lines, columns and spans as doubles, routes as
# character, and an absent field as NA or NULL. A truth row names its file
# relative to `directory`; a location names it by its absolute path
# (P-ISSUE-1), so `directory` is the absolute directory the test wrote it in.
issue_truth <- function(expected, directory) {
  text <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_character_ else issue[[field]], "")
  number <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_real_ else as.double(issue[[field]]), 0)
  list(severity = text("severity"), phase = text("phase"), condition = text("condition"),
    rule = text("rule"),
    spec_paths = lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
    file = vapply(expected, function(issue)
      if (is.null(issue$file)) NA_character_ else file.path(directory, issue$file), ""),
    line = number("line"), column = number("column"),
    message = enc2utf8(text("message")), context = enc2utf8(text("context")),
    source_span = lapply(expected, function(issue)
      if (is.null(issue$source_span)) NULL else as.double(unlist(issue$source_span))),
    operand_route = lapply(expected, function(issue)
      if (is.null(issue$operand_route)) NULL else as.character(unlist(issue$operand_route))))
}
# The twelve columns of an issues table as one list, to compare with
# issue_truth().
issue_columns_of <- function(frame) {
  list(severity = frame$severity, phase = frame$phase, condition = frame$condition,
    rule = frame$rule, spec_paths = frame$spec_paths, file = frame$file, line = frame$line,
    column = frame$column, message = frame$message, context = frame$context,
    source_span = frame$source_span, operand_route = frame$operand_route)
}
# Each row of an issues table against the authored rows of the shared truth:
# its phase, condition, rule (absent as NA), spec paths and decoded context,
# with key order ignored. Where `columns` is given, the table has exactly
# those columns. Needs jsonlite and context.R.
expect_issue_rows <- function(frame, expected, label, columns = NULL) {
  if (!is.null(columns)) expect_identical(names(frame), columns, paste0(label, ": the issue columns"))
  expect_identical(nrow(frame), length(expected), paste0(label, ": the number of issues"))
  for (index in seq_along(expected)) {
    row <- expected[[index]]
    where <- sprintf("%s: issue %d", label, index)
    expect_identical(
      list(phase = frame$phase[[index]], condition = frame$condition[[index]],
        rule = frame$rule[[index]], spec_paths = frame$spec_paths[[index]]),
      list(phase = row$phase, condition = row$condition,
        rule = if (is.null(row$rule)) NA_character_ else row$rule,
        spec_paths = as.character(unlist(row$spec_paths, use.names = FALSE))),
      where)
    expect_context(jsonlite::fromJSON(frame$context[[index]], simplifyVector = FALSE),
      row$context, paste0(where, ": its context"))
  }
}

# Run `code` with the yamaa namespace's binding `name` replaced by `value`,
# and restore the original afterwards.
with_binding <- function(name, value, code) {
  namespace <- asNamespace("yamaa")
  original <- get(name, namespace)
  replace <- function(value) {
    unlockBinding(name, namespace)
    assign(name, value, namespace)
    lockBinding(name, namespace)
  }
  on.exit(replace(original))
  replace(value)
  force(code)
}
# Run `code` with the host capabilities `verify` and `resolve` in place of the
# installed ones.
with_capabilities <- function(verify, resolve, code) {
  with_binding(".yamaa_locked_host_capabilities",
    function() list(verify = verify, resolve = resolve), code)
}

# A run-identity label quoted as JSON text; no semantic field is normalized.
json_text <- function(value) {
  characters <- strsplit(enc2utf8(value), "", fixed = TRUE)[[1L]]
  escaped <- vapply(characters, function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code), "8" = "\\b", "9" = "\\t", "10" = "\\n", "12" = "\\f", "13" = "\\r",
      "34" = "\\\"", "92" = "\\\\", if (code < 32L) sprintf("\\u%04x", code) else character)
  }, character(1L), USE.NAMES = FALSE)
  paste0('"', paste0(escaped, collapse = ""), '"')
}
# An independently authored run report, as JSON text, relabelled as this R
# run's: its runtime, R's version and the engine's version; its example is
# "domain", and each `"specification":` naming `written` names `entry`.
# Every semantic finding and read stays exact.
r_report_text <- function(text, example, entry, written = "spec.yaml") {
  text <- sub('"runtime":"python"', '"runtime":"r"', text, fixed = TRUE)
  text <- sub("fixture-runtime", as.character(getRversion()), text, fixed = TRUE)
  text <- sub("fixture-engine", yamaa:::.core_version(), text, fixed = TRUE)
  text <- sub(paste0('"example":', json_text(example)), '"example":"domain"', text, fixed = TRUE)
  gsub(paste0('"specification":', json_text(written)),
    paste0('"specification":', json_text(entry)), text, fixed = TRUE)
}
# The report a run gives before its save: no artifacts yet.
unsaved_report_text <- function(text) {
  sub('^\\{"artifacts":.*,"backend":', '{"artifacts":[],"backend":', text)
}

# Hide every program but rm from PATH, so that the installed runtime cannot
# reach Python. Returns the function that checks it still cannot and restores
# PATH.
without_python <- function() {
  original <- Sys.getenv("PATH")
  runtime <- tempfile("native-only-path-")
  dir.create(runtime)
  # R itself may invoke rm during temporary-file cleanup; keep only that utility.
  rm_path <- Sys.which("rm")
  if (nzchar(rm_path)) invisible(file.symlink(rm_path, file.path(runtime, "rm")))
  Sys.setenv(PATH = runtime)
  no_python <- function() !nzchar(Sys.which("python")) && !nzchar(Sys.which("python3"))
  stopifnot("the runtime's PATH holds no Python" = no_python())
  function() {
    stopifnot("the runtime's PATH still holds no Python" = no_python())
    Sys.setenv(PATH = original)
    unlink(runtime, recursive = TRUE)
  }
}
