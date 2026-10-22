# Shared helpers of the installed R original-specifications suites,
# R/yamaa/tests/original_specifications*.R. Each suite sources reader.R,
# context.R and assert.R first, skips when jsonlite is not installed, and
# attaches yamaa; this file then reads the family's truth from cases.json and
# the staged independent owners under specification-original.
host_truth <- read_host_fixture(host_fixture_root, "original-specifications")
root <- normalizePath("specification-original", winslash = "/", mustWork = TRUE)

# The public API runs or checks one specification; only a result writes.
run_spec <- function(path) yamaa_spec(path)$run()
check_spec <- function(path) yamaa_spec(path)$check()
# Private installed-run evidence: the run's complete observations as JSON text,
# or NULL for a specification refused before its run began.
domain_observations <- function(result) yamaa:::.run_observations(result)
# save() returns the paths it wrote, invisibly. A refused save signals
# yamaa_error with the run's own issues: a failed run's, or a successful run's
# refused publication (save_refused() in assert.R).
saved <- function(result) returns_paths(result$save())

# Every public issues table has the twelve columns of D28; the authored
# finding fields come among them in column order. Truth compares the fields
# it authors.
issue_columns <- c("severity", "phase", "condition", "rule", "spec_paths", "file", "line", "column",
  "message", "context", "source_span", "operand_route")
authored_columns <- fixture_strings(host_truth[["issue_columns"]])
in_column_order <- function(fields) identical(fields, issue_columns[issue_columns %in% fields])
stopifnot("the authored issue fields come in column order" = in_column_order(authored_columns))
check_issue_frame <- function(actual, expected, label = "the issues") {
  fields <- names(expected)
  stopifnot_case(label, "the issues are a data frame" = is.data.frame(actual),
    "the truth authors every shared field" = all(authored_columns %in% fields),
    "the truth's fields come in column order" = in_column_order(fields))
  expect_identical(names(actual), issue_columns, paste(label, "have the twelve columns"))
  expect_identical(lapply(actual[fields], identity), expected, paste(label, "are the authored findings"))
  stopifnot_case(label,
    "the text columns are character" = all(vapply(actual[c("severity", "phase", "condition", "rule", "file",
      "message", "context")], is.character, logical(1))),
    "line and column are double" = all(vapply(actual[c("line", "column")], is.double, logical(1))),
    "every issue is an error" = all(actual$severity == "error"),
    "every issue has a message" = all(nzchar(actual$message)),
    "spec paths, spans and routes are list columns" =
      all(vapply(actual[c("spec_paths", "source_span", "operand_route")], is.list, logical(1))))
}
# One issue row's phase, condition, rule, spec paths and context, compared
# with the fields of a row of an authored TSV.
expect_tsv_issue <- function(actual, finding, label, context = actual$context) {
  expect_identical(
    list(phase = actual$phase, condition = actual$condition, rule = actual$rule,
      spec_paths = actual$spec_paths, context = context),
    list(phase = finding$phase, condition = finding$condition, rule = finding$rule,
      spec_paths = list(finding$spec_path), context = finding$context),
    paste(label, "is the authored finding"))
}
# A finding's context (JSON text) without the layer a refused layer names: a
# schema finding in a 1.x specification names the files the run loaded,
# "source" and "entry" together; a standalone 0.5 one names none (#3422).
without_layer <- function(context) {
  layered <- grepl('"entry":', context, fixed = TRUE)
  context[layered] <- sub(",}$", "}", gsub('"(entry|source)":"[^"]*",?', "", context[layered]))
  context
}
# The lines of an expected/error.yaml that states the one finding of `issues`
# exactly, in any order: its fields, each path as an item and each context
# entry, with a list in flow style; read without a YAML parser (#3422).
error_lines <- function(issues) {
  context <- jsonlite::fromJSON(issues$context[[1L]], simplifyVector = FALSE)
  entries <- vapply(names(context), function(key) {
    value <- context[[key]]
    paste0("  ", key, ": ", if (is.list(value)) paste0("[", paste(unlist(value), collapse = ", "), "]") else value)
  }, character(1), USE.NAMES = FALSE)
  sort(unique(c(paste0("phase: ", issues$phase), paste0("condition: ", issues$condition),
    paste0("rule: ", issues$rule), "spec_paths:", paste0("  - ", issues$spec_paths[[1L]]),
    if (length(context)) "context:", entries)))
}
# One run report replaces the retired verification and warning logs: every
# run that began returns it, first its run row (CHECK-23, CHECK-24).
report_columns <- c("ARTIFACT", "KIND", "SPEC_PATH", "NAME", "ID", "TARGET", "RULE", "SEVERITY",
  "OUTCOME", "CONDITION", "COUNT", "TOTAL", "KEYS", "DETAILS")
check_run_report <- function(result, label) {
  report <- result$report
  succeeded <- !is.null(result$output)
  stopifnot_case(label, "the report is a data frame" = is.data.frame(report),
    "the report has rows" = nrow(report) >= 1L,
    "COUNT and TOTAL are double" = is.double(report$COUNT) && is.double(report$TOTAL))
  expect_identical(names(report), report_columns, paste(label, "has the report columns"))
  expect_identical(as.list(report[1L, c("KIND", "SPEC_PATH", "OUTCOME", "COUNT")]),
    list(KIND = "run", SPEC_PATH = "output", OUTCOME = if (succeeded) "succeeded" else "failed",
      COUNT = if (succeeded) as.double(nrow(result$output)) else NA_real_),
    paste(label, "starts with its run row"))
}
# Exact decimal text of whole doubles, missing kept missing.
integer_text <- function(value) {
  text <- sprintf("%.0f", value)
  text[is.na(value)] <- NA_character_
  text
}
# Decimal integer text beyond 2^53 in magnitude, compared as exact digits.
beyond_double <- function(text) {
  digits <- sub("^-", "", text)
  !is.na(text) & (nchar(digits) > 16L | (nchar(digits) == 16L & digits > "9007199254740992"))
}
# Compare an output with authored text cells by declared kind: str text and
# int decimal text. R exchanges int as double, so an int cell beyond 2^53
# makes the output view refuse, naming the first such cell column by column;
# the run's saved bytes keep every value.
check_text_columns <- function(result, truth, label) {
  unheld <- NULL
  for (column in names(truth$columns)) if (is.null(unheld) && truth$kinds[[column]] == "int") {
    beyond <- beyond_double(truth$columns[[column]])
    if (any(beyond)) unheld <- truth$columns[[column]][which(beyond)[[1L]]]
  }
  if (!is.null(unheld)) {
    failure <- tryCatch(result$output, error = identity)
    stopifnot_case(label, "the output view signals yamaa_error" = inherits(failure, "yamaa_error"))
    expect_identical(conditionMessage(failure),
      paste0("integer ", unheld, " is beyond 2^53 and cannot be held exactly by an R double"),
      paste(label, "refuses the first unheld integer"))
    return(invisible())
  }
  expect_identical(names(result$output), names(truth$columns), paste(label, "has the authored columns"))
  for (column in names(truth$columns)) {
    actual <- result$output[[column]]
    where <- paste(label, "column", column)
    if (truth$kinds[[column]] == "int") {
      stopifnot_case(where, "an int column is a plain double" = is.double(actual) && is.null(attributes(actual)))
      actual <- integer_text(actual)
    }
    else stopifnot_case(where, "a str column is character" = is.character(actual))
    expect_identical(as.character(actual), truth$columns[[column]], paste(where, "holds the authored cells"))
  }
}

# A report holds two handler ledgers, the run's and its node's: `ledger`, the
# complete authored array, appears as both, byte for byte.
expect_ledger_twice <- function(report, ledger, label) {
  count <- function(text) sum(gregexpr(text, report, fixed = TRUE)[[1L]] > 0L)
  expect_identical(c(count('"handler_counts":'), count(paste0('"handler_counts":', ledger, ','))), c(2L, 2L),
    paste(label, "reports its authored handler ledger as the run's and the node's"))
}
# Compare complete independently authored ledgers while ignoring only JSON
# object key order. Array order, every field, duplicate fields and integer
# text remain part of the comparison; no semantic observation is selected or
# discarded.
check_handler_ledger <- function(report, expected, label = "the report") {
  string <- '"(?:[^"\\\\]|\\\\.)*"'
  field <- paste0(string, ':(?:', string, '|0|[1-9][0-9]*)')
  object <- paste0('\\{', field, '(?:,', field, ')*\\}')
  array <- paste0('\\[(?:', object, '(?:,', object, ')*)?\\]')
  canonical <- function(value) {
    stopifnot_case(label, "a ledger is an array of flat objects" = grepl(paste0('^', array, '$'), value, perl = TRUE))
    objects <- regmatches(value, gregexpr(object, value, perl = TRUE))[[1L]]
    ordered <- vapply(objects, function(value) {
      fields <- regmatches(value, gregexpr(field, value, perl = TRUE))[[1L]]
      paste0('{', paste(sort(fields, method = "radix"), collapse = ','), '}')
    }, character(1L), USE.NAMES = FALSE)
    paste0('[', paste(ordered, collapse = ','), ']')
  }
  prefix <- '"handler_counts":'
  ledgers <- regmatches(report, gregexpr(paste0(prefix, array), report, perl = TRUE))[[1L]]
  stopifnot_case(label, "the report holds two handler ledgers" =
    sum(gregexpr(prefix, report, fixed = TRUE)[[1L]] > 0L) == 2L && length(ledgers) == 2L)
  expect_identical(vapply(substring(ledgers, nchar(prefix) + 1L), canonical, character(1L), USE.NAMES = FALSE),
    rep(canonical(expected), 2L), paste(label, "has the authored handler ledgers"))
}
# The count a run's observations give for `handler`, summed over its sites.
reached_count <- function(observations, handler) {
  counts <- jsonlite::fromJSON(observations, simplifyVector = FALSE)$handler_counts
  sum(vapply(counts, function(item) if (item$handler == handler) item$count else 0, numeric(1)))
}
# Write each of `files`, a named list of text, beneath `directory`.
write_files <- function(directory, files) {
  for (name in names(files)) {
    path <- file.path(directory, name)
    dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
    writeBin(charToRaw(files[[name]]), path)
  }
}
