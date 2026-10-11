# Installed public API contracts; inputs and exact truth are independently authored.
library(yamaa)
check_issues <- function(result) {
  value <- result$issues
  stopifnot(is.data.frame(value),
    identical(names(value), c("phase", "condition", "requirement", "spec_paths", "context")),
    all(vapply(value[c("phase", "condition", "requirement", "context")], is.character, logical(1))),
    is.list(value$spec_paths), all(startsWith(value$context, "{")))
  value
}
argument_failure <- function(code) {
  seen <- tryCatch({ force(code); NULL }, error = identity)
  stopifnot(inherits(seen, "yamaa_argument_error"), is.character(conditionMessage(seen)))
}
stopifnot(identical(names(formals(yamaa_domain)), c("specification", "environment")),
  identical(names(formals(yamaa_check)), c("specification", "environment")),
  identical(formals(yamaa_domain)$environment, NULL),
  identical(formals(yamaa_check)$environment, NULL),
  identical(names(formals(yamaa_define)), c("environment", "standard", "created")))
for (value in list(NULL, 7L, TRUE, NA_character_, character(), c("a", "b"),
  structure("a", class = "path"), structure("a", names = "path"))) {
  argument_failure(yamaa_domain(value))
  argument_failure(yamaa_check(value))
  argument_failure(yamaa_define(value, "sdtm"))
  if (!is.null(value)) {
    argument_failure(yamaa_domain("absent.yaml", environment = value))
    argument_failure(yamaa_check("absent.yaml", environment = value))
  }
}
byte_path <- rawToChar(as.raw(233L)); Encoding(byte_path) <- "bytes"
argument_failure(yamaa_domain(byte_path))
argument_failure(yamaa_define("absent.yaml", 7L))

namespace <- asNamespace("yamaa")
capabilities <- get(".yamaa_locked_host_capabilities", namespace)()
with_binding <- function(name, replacement, code) {
  original <- get(name, namespace)
  replace <- function(value) {
    unlockBinding(name, namespace)
    assign(name, value, namespace)
    lockBinding(name, namespace)
  }
  on.exit(replace(original))
  replace(replacement)
  force(code)
}
with_capabilities <- function(verify, resolve, code) {
  with_binding(".yamaa_locked_host_capabilities",
    function() list(verify = verify, resolve = resolve), code)
}

run_contract <- function() {
  root <- tempfile("yamaa-public-contract-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  write <- function(name, value) writeBin(if (is.raw(value)) value else charToRaw(value), path(name))
  rawfile <- function(name) readBin(path(name), "raw", n = file.info(path(name))$size)
  spec <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
    "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n",
    "  - {name: VALUE, type: int, derivation: SRC.VALUE}\n",
    "output: {path: output.csv, columns: [ID, VALUE]}\n")
  input <- paste0("ID,VALUE\na,-9223372036854775808\nb,9223372036854775807\n",
    "c,9007199254740993\nd,7\ne,\n")
  truth <- c("-9223372036854775808", "9223372036854775807", "9007199254740993", "7", NA_character_)
  write("spec.yaml", spec); write("input.csv", input)
  first <- yamaa_domain(path("spec.yaml")); second <- yamaa_domain(path("spec.yaml"))
  for (result in list(first, second)) {
    stopifnot(nrow(check_issues(result)) == 0L, is.environment(result),
      identical(class(result$output$VALUE), "yamaa_int64_vector"),
      identical(as.character(result$output$VALUE), truth),
      identical(is.na(result$output$VALUE), c(FALSE, FALSE, FALSE, FALSE, TRUE)),
      is.null(result$verification_log), is.null(result$warning_log))
    for (field in c("output", "issues", "verification_log", "warning_log")) {
      stopifnot(inherits(tryCatch({ result[[field]] <- NULL }, error = identity), "error"))
    }
    stopifnot(inherits(tryCatch(result$save(path("other.csv")), error = identity), "error"))
  }
  value <- first$output$VALUE
  stopifnot(identical(as.character(value[c(1L, 5L)]), truth[c(1L, 5L)]),
    identical(as.character(value[[1L]]), truth[[1L]]),
    identical(as.double(value[c(1L, 4L, 5L)]), c(-2^63, 7, NA_real_)),
    identical(as.integer(value[c(4L, 5L)]), c(7L, NA_integer_)),
    inherits(tryCatch(as.double(value[c(2L, 3L)]), error = identity), "error"),
    inherits(tryCatch(as.integer(value[1L]), error = identity), "error"),
    identical(as.character(first$output[c(1L, 5L), , drop = FALSE]$VALUE), truth[c(1L, 5L)]))
  view <- first$output; view$VALUE <- rep("changed", 5L)
  stopifnot(identical(as.character(first$output$VALUE), truth), !file.exists(path("output.csv")))
  unlink(path(c("spec.yaml", "input.csv"))); gc()
  for (attempt in 1:2) stopifnot(identical(first$save(), TRUE), identical(rawfile("output.csv"), charToRaw(input)))
  rm(first); gc()
  stopifnot(identical(as.character(second$output$VALUE), truth), identical(second$save(), TRUE))

  for (result in list(yamaa_check(path("absent.yaml")), yamaa_domain(path("absent.yaml")))) {
    stopifnot(nrow(check_issues(result)) > 0L, is.null(result$spec), is.null(result$resolved_specification))
    if (is.environment(result)) {
      before <- result$issues
      stopifnot(is.null(result$output))
      for (attempt in 1:2) stopifnot(
        inherits(tryCatch(result$save(), error = identity), "yamaa_domain_error"),
        identical(result$issues, before))
    }
  }

  text_spec <- sub("VALUE, type: int", "VALUE, type: str", spec, fixed = TRUE)
  text_spec <- sub("output: {", "  - {name: EMPTY, type: str, derivation: {literal: ''}}\noutput: {", text_spec, fixed = TRUE)
  text_spec <- sub("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]", text_spec, fixed = TRUE)
  write("spec.yaml", text_spec)
  write("input.csv", c(charToRaw("ID,VALUE\na,a"), as.raw(0L), charToRaw("b\nb,"),
    as.raw(c(0xc3, 0xa9)), charToRaw("\nc,\n")))
  text <- yamaa_domain(path("spec.yaml")); stopifnot(nrow(check_issues(text)) == 0L)
  column <- text$output$VALUE
  stopifnot(inherits(column, "yamaa_utf8_vector"),
    identical(unclass(column), list(as.raw(c(97L, 0L, 98L)), as.raw(c(0xc3, 0xa9)), NULL)),
    identical(is.na(column), c(FALSE, FALSE, TRUE)),
    identical(unclass(column[c(1L, 3L)]), list(as.raw(c(97L, 0L, 98L)), NULL)),
    identical(text$output$EMPTY, rep("", 3L)), identical(text$save(), TRUE))
  expected <- c(charToRaw('ID,VALUE,EMPTY\na,a'), as.raw(0L), charToRaw('b,""\nb,'),
    as.raw(c(0xc3, 0xa9)), charToRaw(',""\nc,,""\n'))
  stopifnot(identical(rawfile("output.csv"), expected))

  failed_spec <- sub("  - {name: VALUE, type: int, derivation: SRC.VALUE}",
    paste0("  - name: VALUE\n    type: int\n    derivation: SRC.VALUE\n    verifications:\n",
      "      - range: {max: 0, severity: warning, id: warn}\n",
      "      - range: {max: 0, severity: error, id: fatal}"), spec, fixed = TRUE)
  failed_spec <- sub("output: {", "verifications:\n  - row_count: {min: 0, id: unreached}\noutput: {",
    failed_spec, fixed = TRUE)
  failed_spec <- sub("columns: [ID, VALUE]}",
    "columns: [ID, VALUE], verification_log: checks.csv, warning_log: warnings.csv}",
    failed_spec, fixed = TRUE)
  write("spec.yaml", failed_spec); write("input.csv", "ID,VALUE\na,-3\nb,7\nc,\n")
  for (name in c("output.csv", "warnings.csv")) write(name, "sentinel")
  failed <- yamaa_domain(path("spec.yaml")); issues <- check_issues(failed)
  stopifnot(identical(issues$condition, "range_failed"), is.null(failed$output),
    is.null(failed$warning_log),
    identical(failed$verification_log$VERIFICATION_ID, c("warn", "fatal")),
    identical(failed$verification_log$OUTCOME, c("violated", "violated")),
    identical(as.character(failed$verification_log$EVALUATED_COUNT), c("3", "3")),
    identical(as.character(failed$verification_log$FAILURE_COUNT), c("1", "1")),
    identical(failed$verification_log$REQUIREMENT, c("REQ-0377", "REQ-0377")))
  log <- failed$verification_log
  changed <- log; changed$REPORT_VERSION[] <- "changed"
  unlink(path(c("spec.yaml", "input.csv"))); gc()
  for (attempt in 1:2) stopifnot(
    inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"),
    identical(failed$issues, issues), identical(failed$verification_log, log),
    identical(rawfile("output.csv"), charToRaw("sentinel")),
    identical(rawfile("warnings.csv"), charToRaw("sentinel")),
    length(readLines(path("checks.csv"))) == 3L)
  rm(failed); gc()
  stopifnot(identical(log$VERIFICATION_ID, c("warn", "fatal")))

  # Reenter public facades from an installed function callback; the callback
  # adapter is replaced only to inject behavior, never to inspect native state.
  write("spec.yaml", spec); write("input.csv", input)
  original <- system.file("project-original", "negative-function-contract", package = "yamaa", mustWork = TRUE)
  stopifnot(file.copy(original, root, recursive = TRUE))
  project <- path("negative-function-contract")
  entry <- file.path(project, "spec.yaml")
  source <- readBin(entry, "raw", n = file.info(entry)$size)
  writeBin(charToRaw(sub("args: {}", "args: {x: SOURCE.VALUE}", rawToChar(source), fixed = TRUE)), entry)
  environment <- file.path(project, "r", "environment.yaml")
  interrupted <- structure(list(message = "public contract activation interrupt", call = NULL),
    class = c("public_contract_interrupt", "interrupt", "condition"))
  seen <- with_binding(".yamaa_verify_locked_versions", function(...) stop(interrupted),
    tryCatch(yamaa_domain(entry, environment), interrupt = identity))
  stopifnot(identical(seen, interrupted), !file.exists(file.path(project, "test.csv")))
  calls <- list()
  resolve_reentrant <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(encoded) {
      checked <- yamaa_check(path("spec.yaml"))
      nested <- yamaa_domain(path("spec.yaml"))
      stopifnot(nrow(check_issues(checked)) == 0L, nrow(check_issues(nested)) == 0L,
        identical(as.character(nested$output$VALUE), truth))
      calls[[length(calls) + 1L]] <<- nested
      invoke(encoded)
    })
  }
  outer <- with_capabilities(capabilities$verify, resolve_reentrant,
    yamaa_domain(entry, environment))
  stopifnot(nrow(check_issues(outer)) == 0L, length(calls) >= 3L)
  unlink(path(c("spec.yaml", "input.csv"))); gc()
  stopifnot(identical(as.character(calls[[1L]]$output$VALUE), truth))

  defined <- yamaa_define(path("absent-environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
  stopifnot(nrow(check_issues(defined)) > 0L,
    inherits(tryCatch(defined$save(), error = identity), "yamaa_define_error"))
  invisible(NULL)
}
run_contract()
cat("public signatures, argument errors, owned results and logs, exact values, interrupts and reentrancy passed\n")
