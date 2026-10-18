# Installed public API contracts; inputs and exact truth are independently authored.
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-contract")
stopifnot(identical(as.character(packageVersion("yamaa")), "0.4.0"),
  identical(yamaa:::.core_version(), as.character(packageVersion("yamaa"))))
stopifnot(identical(sort(getNamespaceExports("yamaa")), c("yamaa_check", "yamaa_define", "yamaa_domain",
  "yamaa_memory_resolver", "yamaa_read_resolved_specification", "yamaa_resolve_specification",
  "yamaa_resource_failure", "yamaa_resource_resolver", "yamaa_resource_result",
  "yamaa_synthetic_remote_resolver")))
# A record context value against the decoded frame context: objects are named
# lists (an empty object keeps empty names), arrays unnamed lists, integers of
# magnitude at most 2^53 doubles, wider integer tokens the exact decimal text
# that also appears verbatim in the frame's JSON, other numbers doubles. A
# wider token is matched through the same decoder that produced the expected
# value, never through as.numeric: a host string-to-double conversion can
# mis-round a 19-20 digit token (observed on macOS, where the exact token
# -9223372036854775808 parses one ulp past -2^63), while the decoder parses
# the same token to the value it produced from the frame text.
record_value_matches <- function(actual, expected, text) {
  if (is.null(expected)) return(is.null(actual))
  if (is.list(expected)) {
    if (!is.list(actual) || length(actual) != length(expected)) return(FALSE)
    if (is.null(names(expected))) {
      if (!is.null(names(actual))) return(FALSE)
      return(all(vapply(seq_along(expected), function(index)
        record_value_matches(actual[[index]], expected[[index]], text), logical(1))))
    }
    if (is.null(names(actual)) || !setequal(names(actual), names(expected))) return(FALSE)
    return(all(vapply(names(expected), function(key)
      record_value_matches(actual[[key]], expected[[key]], text), logical(1))))
  }
  if (is.character(expected) || is.logical(expected)) return(identical(actual, expected))
  if (is.integer(expected)) return(identical(actual, as.double(expected)))
  if (!is.double(expected)) return(FALSE)
  if (abs(expected) <= 2^53) return(identical(actual, expected))
  if (is.double(actual)) return(identical(actual, expected))
  is.character(actual) && length(actual) == 1L && grepl("^-?[0-9]+$", actual) &&
    isTRUE(identical(jsonlite::fromJSON(actual), expected)) && grepl(actual, text, fixed = TRUE)
}
check_projection <- function(frame, index, record) {
  stopifnot(is.list(record),
    identical(names(record), fixture_strings(host_truth[["diagnostic_record_names"]])),
    identical(record$phase, frame$phase[[index]]),
    identical(record$condition, frame$condition[[index]]),
    identical(record$requirement,
      if (is.na(frame$requirement[[index]])) NULL else frame$requirement[[index]]),
    identical(record$spec_paths, frame$spec_paths[[index]]),
    is.list(record$context), !is.null(names(record$context)),
    record_value_matches(record$context,
      jsonlite::fromJSON(frame$context[[index]], simplifyVector = FALSE), frame$context[[index]]),
    is.null(record$source_span) || (is.list(record$source_span) &&
      identical(names(record$source_span), c("start", "end")) &&
      is.double(record$source_span$start) && is.double(record$source_span$end) &&
      record$source_span$start >= 0 && record$source_span$start <= record$source_span$end),
    is.null(record$operand_route) || is.character(record$operand_route))
}
check_issues <- function(result) {
  value <- result$issues
  stopifnot(is.data.frame(value),
    identical(names(value), fixture_strings(host_truth[["issue_columns"]])),
    all(vapply(value[c("phase", "condition", "requirement", "context")], is.character, logical(1))),
    is.list(value$spec_paths), all(startsWith(value$context, "{")))
  records <- result$diagnostics
  stopifnot(is.list(records), is.null(names(records)), length(records) == nrow(value))
  for (index in seq_along(records)) check_projection(value, index, records[[index]])
  value
}
argument_failure <- function(code) {
  seen <- tryCatch({ force(code); NULL }, error = identity)
  stopifnot(inherits(seen, "yamaa_argument_error"), is.character(conditionMessage(seen)))
}
stopifnot(identical(names(formals(yamaa_domain)), c("specification", "environment", "resolver")),
  identical(names(formals(yamaa_check)), c("specification", "environment", "resolver")),
  identical(formals(yamaa_domain)$environment, NULL),
  identical(formals(yamaa_check)$environment, NULL),
  identical(formals(yamaa_domain)$resolver, NULL),
  identical(formals(yamaa_check)$resolver, NULL),
  identical(names(formals(yamaa_define)), fixture_strings(host_truth[["define_argument_names"]])))
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
  spec <- host_truth[["specification_yaml"]]
  input <- host_truth[["r_input_csv"]]
  # R holds plain doubles and character: the full-range integers here have no
  # exact R value, so reading the frame raises rather than change one, while
  # the retained bytes still save exactly.
  refuses_output <- function(result, reason) {
    seen <- tryCatch(result$output, error = identity)
    inherits(seen, "error") && grepl(reason, conditionMessage(seen), fixed = TRUE)
  }
  beyond <- "is beyond 2^53 and cannot be held exactly by an R double"
  write("spec.yaml", spec); write("input.csv", input)
  first <- yamaa_domain(path("spec.yaml")); second <- yamaa_domain(path("spec.yaml"))
  for (result in list(first, second)) {
    stopifnot(nrow(check_issues(result)) == 0L, is.environment(result),
      refuses_output(result, beyond),
      is.null(result$verification_log), is.null(result$warning_log))
    for (field in fixture_strings(host_truth[["owned_view_names"]])) {
      stopifnot(inherits(tryCatch({ result[[field]] <- NULL }, error = identity), "error"))
    }
    stopifnot(inherits(tryCatch(result$save(path("other.csv")), error = identity), "error"))
  }
  stopifnot(!file.exists(path("output.csv")))
  unlink(path(c("spec.yaml", "input.csv"))); gc()
  for (attempt in 1:2) stopifnot(identical(first$save(), TRUE), identical(rawfile("output.csv"), charToRaw(input)))
  rm(first); gc()
  stopifnot(refuses_output(second, beyond), identical(second$save(), TRUE))

  for (result in list(yamaa_check(path("absent.yaml")), yamaa_domain(path("absent.yaml")))) {
    stopifnot(nrow(check_issues(result)) > 0L, is.null(result$spec), is.null(result$resolved_specification),
      identical(vapply(result$diagnostics, `[[`, character(1), "condition"), result$issues$condition))
    if (is.environment(result)) {
      before <- result$issues
      records <- result$diagnostics
      stopifnot(is.null(result$output))
      for (attempt in 1:2) stopifnot(
        inherits(tryCatch(result$save(), error = identity), "yamaa_domain_error"),
        identical(result$issues, before), identical(result$diagnostics, records))
    }
  }

  text_spec <- sub("VALUE, type: int", "VALUE, type: str", spec, fixed = TRUE)
  text_spec <- sub("output: {", host_truth[["empty_text_column"]], text_spec, fixed = TRUE)
  text_spec <- sub("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]", text_spec, fixed = TRUE)
  write("spec.yaml", text_spec)
  write("input.csv", c(charToRaw("ID,VALUE\na,a"), as.raw(0L), charToRaw("b\nb,"),
    as.raw(c(0xc3, 0xa9)), charToRaw("\nc,\n")))
  text <- yamaa_domain(path("spec.yaml")); stopifnot(nrow(check_issues(text)) == 0L)
  # Text holding NUL has no R character value either: the frame refuses it and
  # Save still publishes the held bytes.
  stopifnot(refuses_output(text, "text containing NUL cannot be held by an R character vector"),
    identical(text$save(), TRUE), nrow(check_issues(text)) == 0L)
  expected <- c(charToRaw('ID,VALUE,EMPTY\na,a'), as.raw(0L), charToRaw('b,""\nb,'),
    as.raw(c(0xc3, 0xa9)), charToRaw(',""\nc,,""\n'))
  stopifnot(identical(rawfile("output.csv"), expected))

  failed_spec <- sub(host_truth[["integer_column_yaml"]],
    host_truth[["fatal_integer_column_yaml"]], spec, fixed = TRUE)
  failed_spec <- sub("output: {", host_truth[["unreached_row_count_yaml"]],
    failed_spec, fixed = TRUE)
  failed_spec <- sub("columns: [ID, VALUE]}",
    host_truth[["log_output_yaml"]],
    failed_spec, fixed = TRUE)
  write("spec.yaml", failed_spec); write("input.csv", "ID,VALUE\na,-3\nb,7\nc,\n")
  for (name in c("output.csv", "warnings.csv")) write(name, "sentinel")
  failed <- yamaa_domain(path("spec.yaml")); issues <- check_issues(failed)
  # The same failure through the named record.
  records <- failed$diagnostics
  stopifnot(length(records) == 1L, identical(records[[1L]]$condition, "range_failed"),
    identical(records[[1L]]$phase, issues$phase), identical(records[[1L]]$requirement, issues$requirement),
    identical(records[[1L]]$spec_paths, issues$spec_paths[[1L]]), is.list(records[[1L]]$context))
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
    identical(failed$issues, issues), identical(failed$diagnostics, records),
    identical(failed$verification_log, log),
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
  for (stage in c("lock","binding","conformance","derivation")) {
    interrupted <- structure(list(message = host_truth[["interrupt_message"]], call = NULL, token = new.env()),
      class = c("public_contract_interrupt", "interrupt", "condition"))
    invoked <- 0L
    verify <- function(...) {
      if (stage=="lock") return(list(3L,interrupted))
      capabilities$verify(...)
    }
    resolve <- function(call,parameters) {
      if (stage=="binding") return(list(3L,interrupted))
      resolved <- capabilities$resolve(call,parameters)
      if (resolved[[1L]]!=0L) return(resolved)
      invoke <- resolved[[2L]]
      list(0L,function(arguments) {
        invoked <<- invoked+1L
        if (stage=="conformance" || (stage=="derivation" && invoked>2L)) return(list(3L,interrupted))
        invoke(arguments)
      })
    }
    seen <- with_capabilities(verify,resolve,tryCatch(yamaa_domain(entry,environment),interrupt=identity))
    expected <- if (stage=="conformance") 1L else if (stage=="derivation") 3L else 0L
    stopifnot(identical(seen,interrupted),identical(invoked,expected),!file.exists(file.path(project,"test.csv")))
  }
  long_condition <- structure(list(message=paste0(strrep("\u00e9\n",30000L),"complete failure"),call=NULL),
    class=c("public_contract_error","error","condition"))
  invoked <- 0L
  resolve_long <- function(call,parameters) {
    resolved <- capabilities$resolve(call,parameters)
    if(resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L,function(arguments) {
      invoked <<- invoked+1L
      if(invoked>2L) return(list(1L,long_condition))
      invoke(arguments)
    })
  }
  long_failed <- with_capabilities(capabilities$verify,resolve_long,yamaa_domain(entry,environment))
  stopifnot(identical(invoked,3L),is.null(long_failed$output),
    identical(long_failed$issues$condition,"function_call_failed"))
  context <- jsonlite::fromJSON(long_failed$issues$context[[1L]],simplifyVector=FALSE)
  stopifnot(identical(context$host_error,"public_contract_error"),
    identical(context$host_message,long_condition$message),is.null(context$host_details_truncated))
  calls <- list()
  resolve_reentrant <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      checked <- yamaa_check(path("spec.yaml"))
      nested <- yamaa_domain(path("spec.yaml"))
      stopifnot(nrow(check_issues(checked)) == 0L, nrow(check_issues(nested)) == 0L,
        refuses_output(nested, beyond))
      calls[[length(calls) + 1L]] <<- nested
      invoke(arguments)
    })
  }
  outer <- with_capabilities(capabilities$verify, resolve_reentrant,
    yamaa_domain(entry, environment))
  stopifnot(nrow(check_issues(outer)) == 0L, length(calls) >= 3L)
  original_condition <- structure(list(message = "held output formatter failure", call = NULL),
    class = c("public_contract_error", "error", "condition"))
  writeBin(charToRaw("ID,VALUE\n01,1.234\n"), file.path(project, "input", "source.csv"))
  resolve_failed <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      if (identical(arguments[[1L]], 1.234)) {
        return(list(1L, original_condition))
      }
      invoke(arguments)
    })
  }
  failed <- with_binding(".project_condition_details", function(condition) list(),
    with_capabilities(capabilities$verify, resolve_failed,
      yamaa_domain(entry, environment)))
  measured <- rawToChar(get(".domain_observations", namespace)(failed,
    include_measurements = TRUE))
  stopifnot(is.null(failed$output),
    grepl('"code":"opaque_host"', failed$issues$context, fixed = TRUE),
    grepl('"outcome":"failed","phase":"output_preparation"', measured, fixed = TRUE),
    inherits(tryCatch(failed$save(), error = identity), "yamaa_domain_error"))
  unlink(path(c("spec.yaml", "input.csv"))); gc()
  stopifnot(refuses_output(calls[[1L]], beyond))

  defined <- yamaa_define(path("absent-environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
  stopifnot(nrow(check_issues(defined)) > 0L)
  define_error <- tryCatch(defined$save(), error = identity)
  stopifnot(inherits(define_error, "yamaa_define_error"), identical(define_error$issues, defined$issues),
    identical(define_error$diagnostics, defined$diagnostics))
  invisible(NULL)
}
run_contract()

qualify_diagnostic_records <- function() {
  root <- tempfile("yamaa-diagnostic-records-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  # A literal beyond every 64-bit range is refused before study access; its
  # context carries the signed 64-bit bounds and the exact literal, which R
  # keeps as decimal text because binary64 cannot hold them.
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: N, type: int, derivation: {literal: 18446744073709551616}}",
    "output: {path: output.csv, columns: [ID, N]}"), path("spec.yaml"))
  checked <- yamaa_check(path("spec.yaml")); frame <- check_issues(checked)
  stopifnot(length(checked$diagnostics) == 1L)
  record <- checked$diagnostics[[1L]]
  stopifnot(identical(record$phase, "validation"), identical(record$condition, "integer_overflow"),
    identical(record$requirement, "REQ-0014"), identical(record$spec_paths, "columns.N.derivation.literal"),
    identical(record$context[order(names(record$context))],
      list(actual = "18446744073709551616", maximum = "9223372036854775807", minimum = "-9223372036854775808")),
    grepl('"actual":18446744073709551616', frame$context[[1L]], fixed = TRUE),
    is.null(record$source_span), is.null(record$operand_route))
  # A division by zero fails at one node of the compute expression: the
  # record spans that node's bytes and routes to it from the root, so a
  # failure at the root has a present empty route, never NULL.
  writeBin(charToRaw("ID,VALUE,ZERO\na,2.5,0\n"), path("input.csv"))
  for (case in list(list(expression = "1 + VALUE / ZERO", route = "right"),
                    list(expression = "VALUE / ZERO", route = character(0)))) {
    writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
      "input: {SRC: {path: input.csv, types: {ID: str, VALUE: float, ZERO: float}}}",
      "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
      "  - {name: VALUE, type: float, derivation: SRC.VALUE}",
      "  - {name: ZERO, type: float, derivation: SRC.ZERO}",
      paste0("  - {name: RATIO, type: float, derivation: {compute: {expr: '", case$expression, "'}}}"),
      "output: {path: output.csv, columns: [ID, VALUE, ZERO, RATIO]}"), path("spec.yaml"))
    result <- yamaa_domain(path("spec.yaml")); check_issues(result)
    stopifnot(is.null(result$output), length(result$diagnostics) == 1L)
    record <- result$diagnostics[[1L]]
    span <- record$source_span
    stopifnot(identical(record$phase, "derivation"), identical(record$condition, "division_by_zero"),
      identical(record$requirement, "REQ-0430"), identical(record$spec_paths, "columns.RATIO.derivation.compute"),
      identical(record$context$expr, case$expression), identical(record$context$keys, list(list(ID = "a"))),
      is.list(span), identical(names(span), c("start", "end")), is.double(span$start), is.double(span$end),
      identical(substr(case$expression, span$start + 1, span$end), "VALUE / ZERO"),
      identical(record$operand_route, case$route), !is.null(record$operand_route),
      identical(result$diagnostics, list(record)))
  }
  cat("diagnostic records keep exact wide integers and distinguish recorded from absent geometry\n")
}
qualify_diagnostic_records()

# Python reads the same cases, and both hosts equal this one truth. Numbers
# are tagged ({"integer": text} or {"float": text}) and text holding NUL is
# {"hex": bytes}, so no JSON reader chooses a number type. Under this host's
# documented rules an integer of magnitude at most 2^53 is a double, a wider
# one keeps its exact decimal text, and text holding NUL is a yamaa_utf8 raw.
parity_value <- function(value) {
  # A number token is read by jsonlite, not as.numeric: R's own conversion
  # can mis-round a long token (seen on macOS).
  tag <- if (is.list(value) && length(value) == 1L) names(value) else NULL
  if (identical(tag, "integer")) {
    digits <- sub("^-", "", value$integer)
    exact <- nchar(digits) < 16L || (nchar(digits) == 16L && digits <= "9007199254740992")
    return(if (exact) as.double(jsonlite::fromJSON(value$integer)) else value$integer)
  }
  if (identical(tag, "float")) return(as.double(jsonlite::fromJSON(value$float)))
  if (identical(tag, "hex")) {
    bytes <- fixture_bytes(value)
    if (any(bytes == as.raw(0L))) return(structure(bytes, class = "yamaa_utf8"))
    return(enc2utf8(rawToChar(bytes)))
  }
  if (is.list(value)) return(lapply(value, parity_value))
  stopifnot(is.null(value) || is.character(value) || is.logical(value))
  value
}
parity_record <- function(truth) {
  span <- truth$source_span
  list(phase = truth$phase, condition = truth$condition, requirement = truth$requirement,
    spec_paths = as.character(unlist(truth$spec_paths)),
    context = parity_value(truth$context),
    source_span = if (is.null(span)) NULL else
      list(start = as.numeric(span[[1L]]), end = as.numeric(span[[2L]])),
    operand_route = if (is.null(truth$operand_route)) NULL else
      as.character(unlist(truth$operand_route)))
}
qualify_diagnostic_parity <- function() {
  root <- tempfile("yamaa-diagnostic-parity-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  for (case in read_host_fixture(host_fixture_root, "diagnostic-parity")$cases) {
    directory <- file.path(root, case$name); dir.create(directory)
    spec <- file.path(directory, "spec.yaml")
    if (!is.null(case$specification_yaml)) {
      writeBin(charToRaw(enc2utf8(case$specification_yaml)), spec)
      writeBin(fixture_bytes(case$input_csv), file.path(directory, "input.csv"))
    }
    result <- switch(case$entry, check = yamaa_check(spec), domain = yamaa_domain(spec))
    frame <- result$issues
    expected <- lapply(case$records, parity_record)
    if (!identical(result$diagnostics, expected)) {
      str(result$diagnostics); str(expected)
      stop("diagnostic records differ from the shared parity truth: ", case$name)
    }
    # The frame is the projection of the same records. jsonlite cuts decoded
    # text at an escaped NUL, so a context holding one is compared above only.
    if (!any(grepl("\\u0000", frame$context, fixed = TRUE))) check_issues(result)
    stopifnot(identical(frame$phase, vapply(expected, `[[`, "", "phase")),
      identical(frame$condition, vapply(expected, `[[`, "", "condition")),
      identical(frame$requirement, vapply(expected, function(record)
        if (is.null(record$requirement)) NA_character_ else record$requirement, "")),
      identical(frame$spec_paths, lapply(expected, `[[`, "spec_paths")))
  }
  cat("diagnostic records equal the shared parity truth field for field\n")
}
qualify_diagnostic_parity()
cat("public signatures, argument errors, owned results and logs, exact values, interrupts and reentrancy passed\n")

qualify_public_large_text_callback <- function() {
  directory <- tempfile("public-large-text-callback-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  lock <- charToRaw(paste0('{"R":{"Version":"',getRversion(),'"},"Packages":{"yamaa":{"Version":"',utils::packageVersion("yamaa"),'"}}}'))
  writeBin(lock,file.path(directory,"renv.lock"))
  writeBin(charToRaw("schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions:\n  text:\n    function: base::identity\n    description: Return text unchanged.\n    params: [{name: x, type: str}]\n    returns: str\n    tests:\n      - {id: normal, covers: [normal], args: {x: short}, result: short}\n      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"),file.path(directory,"environment.yaml"))
  writeBin(charToRaw("schema_version: '1.0'\ndomain: TEXT\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: str}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: V, type: str, derivation: {function: {name: text, args: {x: SRC.V}}}}\noutput: {path: held.csv, columns: [ID, V]}\n"),file.path(directory,"domain.yaml"))
  value <- paste0(strrep("\u00e9",524289L),"complete")
  expected <- charToRaw(paste0("ID,V\n1,",value,"\n"))
  writeBin(expected,file.path(directory,"input.csv"))
  built <- yamaa_domain(file.path(directory,"domain.yaml"),file.path(directory,"environment.yaml"))
  stopifnot(nrow(built$issues)==0L,identical(built$output$V,value))
  unlink(file.path(directory,c("domain.yaml","environment.yaml","input.csv")))
  for(attempt in seq_len(2L)) stopifnot(isTRUE(built$save()),
    identical(readBin(file.path(directory,"held.csv"),"raw",n=length(expected)+1L),expected))
  cat("public callback text beyond one MiB remains owned and saves exact bytes\n")
}
qualify_public_large_text_callback()

qualify_resolved_specification <- function() {
  root <- tempfile("yamaa-resolved-public-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  entry <- file.path(root, "spec.yaml")
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing/raw.csv, types: {ID: int}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: absent/output.csv, columns: [ID]}"), entry)
  artifact <- yamaa_resolve_specification(entry)
  stopifnot(inherits(artifact, "yamaa_resolved_specification"))
  serialized <- as.character(artifact)
  facts <- jsonlite::fromJSON(serialized, simplifyVector = FALSE)
  stopifnot(identical(facts$format, "yamaa.resolved-specification"),
    identical(facts$artifact_version, 1L), identical(facts$entry, entry),
    identical(facts$replay, "inspection_only"),
    !dir.exists(file.path(root, "absent")))
  resource <- Filter(function(value) identical(value$role, "input_data"), facts$entries[[1L]]$resources)[[1L]]
  stopifnot(identical(resource$location, list(kind = "local", path = file.path(root, "missing/raw.csv"))))
  unlink(entry)
  previous <- getwd()
  on.exit(setwd(previous), add = TRUE)
  setwd(dirname(root))
  restored <- yamaa_read_resolved_specification(serialized)
  stopifnot(identical(as.character(restored), serialized))
  error <- tryCatch(yamaa_resolve_specification(entry), error = identity)
  stopifnot(inherits(error, "yamaa_resolved_specification_error"), nrow(error$issues) > 0L,
    identical(vapply(error$diagnostics, `[[`, character(1), "condition"), error$issues$condition))
  invalid <- sub('"artifact_version":1', '"artifact_version":2', serialized, fixed = TRUE)
  stopifnot(inherits(tryCatch(yamaa_read_resolved_specification(invalid), error = identity), "error"))
  cat("resolved specification survives source deletion and working-directory changes\n")
}
qualify_resolved_specification()

qualify_resolved_producer_entries <- function() {
  root <- tempfile("yamaa-resolved-producers-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  dir.create(file.path(root, "shared"))
  writeLines(c("schema_version: '1.0'", "domain: PRODUCER", "keys: [ID]",
    "input: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}",
    "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]",
    "output: {path: output/produced.csv, relative_to: entry, columns: [ID]}"), file.path(root, "shared/producer.yaml"))
  writeLines(c("schema_version: '1.0'", "domain: CONSUMER", "keys: [ID]",
    "input: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}",
    "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]",
    "output: {path: output/final.csv, relative_to: entry, columns: [ID]}"), file.path(root, "shared/consumer.yaml"))
  for (wrapper in c("A", "B")) {
    study <- file.path(root, wrapper)
    dir.create(file.path(study, "input"), recursive = TRUE)
    identifier <- if (identical(wrapper, "A")) "17" else "29"
    writeLines(c("ID", identifier), file.path(study, "input/raw.csv"))
    writeLines(c("schema_version: '1.0'", "parents: ../shared/consumer.yaml"), file.path(study, "spec.yaml"))
    writeLines(c("schema_version: '1.0'", "parents: ../shared/producer.yaml"), file.path(study, "producer.yaml"))
    artifact <- yamaa_resolve_specification(file.path(study, "spec.yaml"))
    entries <- jsonlite::fromJSON(as.character(artifact), simplifyVector = FALSE)$entries
    stopifnot(length(entries) == 2L, identical(entries[[1L]]$producers$P, file.path(study, "producer.yaml")))
    resource <- Filter(function(value) identical(value$role, "input_data"), entries[[2L]]$resources)[[1L]]
    stopifnot(identical(resource$location$path, file.path(study, "input/raw.csv")))
    result <- yamaa_domain(file.path(study, "spec.yaml"))
    stopifnot(nrow(result$issues) == 0L, identical(as.character(result$output$ID), identifier),
      !dir.exists(file.path(study, "output")))
  }
  cat("resolved producer entries agree with direct builds for both studies\n")
}
qualify_resolved_producer_entries()

qualify_resolved_origins <- function() {
  root <- tempfile("yamaa-resolved-origins-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  base <- file.path(root, "base.yaml")
  child <- file.path(root, "child.yaml")
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {RAW: missing.csv}",
    "columns: [{name: ID, type: str, derivation: {mapping: {source: RAW.ID, dict: {A: Alpha}}}}]",
    "output: {path: result.csv, columns: [ID]}"), base)
  writeLines(c("schema_version: '1.0'", "parents: base.yaml"), child)
  origins <- function(path) jsonlite::fromJSON(as.character(yamaa_resolve_specification(path)), simplifyVector = FALSE)$entries[[1L]]$origins
  direct <- origins(base)
  inherited <- origins(child)
  pointer <- "/columns/0/derivation/value/mapping/case_sensitive"
  stopifnot(identical(direct[[pointer]]$source_kind, "schema_default"),
    identical(inherited[[pointer]], direct[[pointer]]), isTRUE(inherited[["/input/RAW"]]$generated))
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {RAW: missing.csv}",
    "windows: {W: {order_by: [RAW.ID]}}", "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: result.csv, columns: [ID]}"), base)
  writeLines(c("schema_version: '1.0'", "parents: base.yaml",
    "columns: [{name: N, type: int, derivation: {row_number: {window: W}}}]",
    "output: {path: result.csv, columns: [ID, N]}"), child)
  inherited <- origins(child)
  prefix <- "/columns/1/derivation/value/row_number/window"
  stopifnot(identical(inherited[[prefix]]$declaring_source, child),
    identical(inherited[[paste0(prefix, "/order_by/0/variable")]]$declaring_source, base),
    identical(inherited[[paste0(prefix, "/order_by/0/direction")]]$source_kind, "schema_default"),
    isTRUE(inherited[["/columns/0/derivation/value/source/variable"]]$generated))
  cat("resolved original origins retain inherited defaults, shorthand and named-window definitions\n")
}
qualify_resolved_origins()
