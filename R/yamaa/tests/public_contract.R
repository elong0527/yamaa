# Installed public API contracts; inputs and exact truth are independently authored.
library(yamaa)
# save() returns the absolute paths it wrote, invisibly, with forward slashes,
# in publication order (D38); each one exists once written.
returns_paths <- function(paths) is.character(paths) && length(paths) > 0L && !anyNA(paths) &&
  all(grepl("^(/|[[:alpha:]]:/)", paths)) && !any(grepl("\\", paths, fixed = TRUE)) &&
  all(file.exists(paths))
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped public_contract.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
projects <- normalizePath("project-base",winslash="/",mustWork=TRUE)
define_fixtures <- normalizePath("define-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-contract")
messages <- host_truth[["messages"]]
# D39's error text the fixture authors under `key`, naming the path given
# and `file`, the file its first issue is located in.
error_text <- function(key, path = NULL, file = NULL) {
  text <- messages[[key]]
  if (!is.null(file)) text <- sub("{file}", file, text, fixed = TRUE)
  if (is.null(path)) text else gsub("{path}", path, text, fixed = TRUE)
}
namespace <- asNamespace("yamaa")

stopifnot(identical(as.character(packageVersion("yamaa")), "0.5.0"),
  identical(yamaa:::.core_version(), as.character(packageVersion("yamaa"))),
  identical(sort(getNamespaceExports("yamaa")),
    c("yamaa_info", "yamaa_lint", "yamaa_project", "yamaa_read_odm", "yamaa_schema",
      "yamaa_selftest", "yamaa_spec")),
  identical(getNamespaceInfo("yamaa", "S3methods")[, 3L],
    c("names.yamaa_check", "length.yamaa_check", "print.yamaa_check", "format.yamaa_check",
      "names.yamaa_run", "length.yamaa_run", "print.yamaa_run", "format.yamaa_run",
      "names.yamaa_member_run", "length.yamaa_member_run", "print.yamaa_member_run",
      "format.yamaa_member_run", "names.yamaa_project_run", "length.yamaa_project_run",
      "[[.yamaa_project_run", "print.yamaa_project_run", "format.yamaa_project_run",
      "names.yamaa_submission", "length.yamaa_submission", "print.yamaa_submission",
      "format.yamaa_submission", "print.yamaa_spec", "format.yamaa_spec", "print.yamaa_project",
      "format.yamaa_project", "names.yamaa_selftest", "length.yamaa_selftest",
      "print.yamaa_selftest", "format.yamaa_selftest")))
for (name in c("yamaa_domain", "yamaa_check", "yamaa_define", "yamaa_resolve_specification",
  "yamaa_read_resolved_specification", "yamaa_resource_resolver", "yamaa_memory_resolver",
  "yamaa_synthetic_remote_resolver", "yamaa_resource_result", "yamaa_resource_failure",
  "yamaa_int64", "yamaa_utf8")) {
  stopifnot(!exists(name, envir = namespace, inherits = FALSE))
}

# The one findings view: the twelve columns of D28, context as canonical JSON
# text, the geometry columns lists holding NULL where nothing was recorded,
# and a location in all three of file, line and column or in none
# (P-ISSUE-1).
check_issues <- function(frame) {
  stopifnot(is.data.frame(frame),
    identical(names(frame), fixture_strings(host_truth[["issue_columns"]])),
    all(vapply(frame[c("severity", "phase", "condition", "rule", "file", "message", "context")],
      is.character, logical(1))),
    all(vapply(frame[c("line", "column")], is.double, logical(1))),
    all(frame$severity == "error"), all(nzchar(frame$message)),
    identical(is.na(frame$file), is.na(frame$line)),
    identical(is.na(frame$file), is.na(frame$column)),
    all(vapply(frame[c("spec_paths", "source_span", "operand_route")], is.list, logical(1))),
    all(vapply(frame$spec_paths, is.character, logical(1))),
    all(startsWith(frame$context, "{")),
    all(vapply(frame$source_span, function(span) is.null(span) || (is.double(span) &&
      length(span) == 2L && span[[1L]] >= 0 && span[[1L]] <= span[[2L]]), logical(1))),
    all(vapply(frame$operand_route, function(route) is.null(route) || is.character(route), logical(1))))
  frame
}
# A failed operation signals yamaa_error carrying the issues that explain it.
refusal <- function(code) {
  seen <- tryCatch({ force(code); NULL }, yamaa_error = identity)
  stopifnot(inherits(seen, "yamaa_error"), inherits(seen, "error"), is.null(conditionCall(seen)))
  check_issues(seen$issues)
  seen
}
# The one issue of a project run asked for ADSL, which it did not build;
# `available` is the canonical JSON of the names it has.
unknown_member <- function(error, available) {
  issue <- error$issues
  nrow(issue) == 1L && identical(issue$phase, "validation") &&
    identical(issue$condition, "unknown_specification_name") &&
    identical(issue$rule, "PROJECT-6") && identical(issue$spec_paths, list(character())) &&
    identical(issue$message, "the project has no specification named ADSL") &&
    identical(issue$context, paste0('{"available":', available, ',"name":"ADSL"}'))
}
# The one issue of a call that selects `section`, which the project does not
# declare; `sections` is the canonical JSON of the sections it does.
unknown_section <- function(error, section, sections) {
  issue <- error$issues
  nrow(issue) == 1L && identical(issue$phase, "validation") &&
    identical(issue$condition, "unknown_section") &&
    identical(issue$rule, "PROJECT-14") && identical(issue$spec_paths, list(character())) &&
    identical(issue$message, paste("the project declares no section", section)) &&
    identical(issue$context, paste0('{"section":"', section, '","sections":', sections, "}"))
}
# The one issue of a save with nothing to write, under `rule`.
nothing_to_save <- function(error, rule) {
  issue <- error$issues
  nrow(issue) == 1L && identical(issue$phase, "output") &&
    identical(issue$condition, "nothing_to_save") && identical(issue$rule, rule) &&
    identical(issue$spec_paths, list(character())) &&
    identical(issue$message, "there is nothing to save") && identical(issue$context, "{}")
}
# Argument mistakes are ordinary errors, never yamaa_error.
argument_failure <- function(code) {
  seen <- tryCatch({ force(code); NULL }, error = identity)
  stopifnot(inherits(seen, "error"), !inherits(seen, "yamaa_error"), is.character(conditionMessage(seen)))
}
members <- function(object) sort(ls(object), method = "radix")
expected_members <- function(kind) fixture_strings(host_truth[["members"]][[kind]])
# names() lists a result's public members in declaration order, never its
# hidden handle, and length() counts them (D44); ls() and `$` still reach
# each of them.
named <- function(object, kind) {
  expected <- fixture_strings(host_truth[["r_names"]][[kind]])
  identical(names(object), expected) && identical(length(object), length(expected)) &&
    setequal(expected, ls(object)) &&
    all(vapply(expected, exists, logical(1), envir = object, inherits = FALSE))
}
arguments_of <- function(f) as.character(names(formals(f)))
expected_arguments <- function(name) as.character(fixture_strings(host_truth[["arguments"]][[name]]))
locked <- function(object, names) {
  for (name in names) {
    stopifnot(inherits(tryCatch({ assign(name, NULL, envir = object); NULL }, error = identity), "error"))
  }
}

qualify_signatures <- function() {
  root <- tempfile("yamaa-signatures-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  spec <- yamaa_spec(file.path(root, "absent.yaml"))
  project <- yamaa_project(file.path(root, "absent-project.yaml"))
  callables <- list(spec = yamaa_spec, project = yamaa_project, read_odm = yamaa_read_odm,
    lint = yamaa_lint, spec.check = spec$check, spec.resolve = spec$resolve, spec.run = spec$run,
    project.spec = project$spec, project.check = project$check, project.run = project$run,
    project.submission = project$submission)
  for (name in names(callables)) {
    if (!identical(arguments_of(callables[[name]]), expected_arguments(name))) stop("arguments differ: ", name)
  }
  limits <- host_truth[["read_odm_limits"]]
  options <- formals(yamaa_read_odm)
  stopifnot(is.null(options$archive_member), is.null(formals(project$run)$section),
    is.null(formals(project$submission)$section), is.null(formals(project$submission)$created))
  for (name in names(limits)) stopifnot(identical(options[[name]], as.numeric(limits[[name]])))
  stopifnot(identical(members(spec), expected_members("spec")),
    identical(members(project), expected_members("project")),
    identical(members(spec$check()), expected_members("check")),
    named(spec$check(), "check"))
  run <- spec$run()
  stopifnot(identical(members(run), expected_members("run")), named(run, "run"),
    is.character(run$status), identical(run[["status"]], run$status),
    identical(arguments_of(run$save), expected_arguments("run.save")))
  locked(spec, c(expected_members("spec"), "extra"))
  locked(project, c(expected_members("project"), "extra"))
  locked(run, c(expected_members("run"), "extra"))
  for (value in list(NULL, 7L, TRUE, NA_character_, character(), c("a", "b"),
    structure("a", class = "path"), structure("a", names = "path"))) {
    argument_failure(yamaa_spec(value))
    argument_failure(yamaa_project(value))
    argument_failure(yamaa_read_odm(value))
    argument_failure(project$spec(value))
    if (!is.null(value)) {
      argument_failure(project$run(value))
      argument_failure(project$submission(value))
      argument_failure(project$submission(created = value))
      argument_failure(yamaa_read_odm(file.path(root, "absent.xml"), archive_member = value))
    }
  }
  byte_path <- rawToChar(as.raw(233L)); Encoding(byte_path) <- "bytes"
  argument_failure(yamaa_spec(byte_path))
  # The removed options are gone, not ignored.
  for (option in c("environment", "resolver")) {
    arguments <- setNames(list(NULL), option)
    argument_failure(do.call(yamaa_spec, c(list(file.path(root, "absent.yaml")), arguments)))
    for (method in list(spec$check, spec$resolve, spec$run, project$check)) {
      argument_failure(do.call(method, arguments))
    }
  }
  argument_failure(run$save(file.path(root, "other.csv")))
  stopifnot(identical(list.files(root), character()))
  cat("public exports, signatures, members, locked results and argument errors passed\n")
}
qualify_signatures()

run_contract <- function() {
  root <- tempfile("yamaa-public-contract-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  write <- function(name, value) writeBin(if (is.raw(value)) value else charToRaw(enc2utf8(value)), path(name))
  rawfile <- function(name) readBin(path(name), "raw", n = file.info(path(name))$size)
  spec <- host_truth[["specification_yaml"]]
  input <- host_truth[["r_input_csv"]]
  # R holds plain doubles and character: the full-range integers here have no
  # exact R value, so reading the frame raises rather than change one, while
  # the held bytes still save exactly.
  refuses_output <- function(result, reason) {
    seen <- tryCatch(result$output, yamaa_error = identity)
    inherits(seen, "yamaa_error") && grepl(reason, conditionMessage(seen), fixed = TRUE) &&
      nrow(check_issues(seen$issues)) == 0L
  }
  beyond <- "is beyond 2^53 and cannot be held exactly by an R double"
  write("spec.yaml", spec); write("input.csv", input)
  first <- yamaa_spec(path("spec.yaml"))$run(); second <- yamaa_spec(path("spec.yaml"))$run()
  for (result in list(first, second)) {
    report <- result$report
    stopifnot(nrow(check_issues(result$issues)) == 0L, is.environment(result),
      refuses_output(result, beyond), is.data.frame(report),
      identical(names(report), fixture_strings(host_truth[["report_columns"]])),
      identical(report$KIND, c("run", "input", "rows", "column", "column")),
      identical(report$OUTCOME[[1L]], "succeeded"), identical(report$COUNT[[1L]], 5))
    locked(result, fixture_strings(host_truth[["read_only_views"]][["run"]]))
  }
  stopifnot(!file.exists(path("output.csv")))
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  # save() returns the one path it wrote, invisibly (D38).
  saved <- withVisible(first$save())
  stopifnot(!saved$visible, identical(saved$value, normalizePath(path("output.csv"), winslash = "/")),
    identical(rawfile("output.csv"), charToRaw(input)))
  rm(first); invisible(gc())
  stopifnot(refuses_output(second, beyond), returns_paths(second$save()))

  absent <- yamaa_spec(path("absent.yaml"))
  checked <- check_issues(absent$check()$issues)
  run <- absent$run()
  # The caller's own path, named as given at no field (FILE-7).
  stopifnot(identical(checked$condition, "resource_path_missing"), identical(checked$rule, "FILE-7"),
    identical(checked$spec_paths, list(character())),
    identical(jsonlite::fromJSON(checked$context, simplifyVector = FALSE), list(path = path("absent.yaml"))),
    is.null(run$output), is.null(run$report), identical(run$issues, checked))
  for (attempt in 1:2) {
    error <- refusal(run$save())
    # A run whose file was not read names the path as given.
    stopifnot(identical(conditionMessage(error), error_text("missing_run_save", path("absent.yaml"))),
      identical(error$issues, checked), identical(run$issues, checked),
      identical(error$paths, character()))
  }
  error <- refusal(absent$resolve())
  stopifnot(identical(conditionMessage(error), error_text("resolve", path("absent.yaml"))),
    identical(error$issues, checked))

  text_spec <- sub("VALUE, type: int", "VALUE, type: str", spec, fixed = TRUE)
  text_spec <- sub("output: {", host_truth[["empty_text_column"]], text_spec, fixed = TRUE)
  text_spec <- sub("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]", text_spec, fixed = TRUE)
  write("spec.yaml", text_spec)
  write("input.csv", c(charToRaw("ID,VALUE\na,a"), as.raw(0L), charToRaw("b\nb,"),
    as.raw(c(0xc3, 0xa9)), charToRaw("\nc,\n")))
  text <- yamaa_spec(path("spec.yaml"))$run(); stopifnot(nrow(check_issues(text$issues)) == 0L)
  # Text holding NUL has no R character value either: the frame refuses it and
  # save still publishes the held bytes.
  stopifnot(refuses_output(text, "text containing NUL cannot be held by an R character vector"),
    returns_paths(text$save()))
  expected <- c(charToRaw('ID,VALUE,EMPTY\na,a'), as.raw(0L), charToRaw('b,""\nb,'),
    as.raw(c(0xc3, 0xa9)), charToRaw(',""\nc,,""\n'))
  stopifnot(identical(rawfile("output.csv"), expected))

  write("spec.yaml", host_truth[["typed_specification_yaml"]])
  write("input.csv", host_truth[["typed_input_csv"]])
  typed <- yamaa_spec(path("spec.yaml"))$run()
  stopifnot(nrow(check_issues(typed$issues)) == 0L)
  frame <- typed$output
  values <- host_truth[["typed_values"]]
  decimal <- function(name) vapply(values[[name]], function(value)
    if (is.null(value)) NA_real_ else as.double(value), double(1), USE.NAMES = FALSE)
  stopifnot(is.data.frame(frame), identical(names(frame), names(values)),
    identical(frame$ID, decimal("ID")), identical(frame$I, decimal("I")),
    identical(frame$F, decimal("F")), identical(1 / frame$F[[1L]], -Inf),
    identical(frame$S, vapply(values$S, function(value) if (is.null(value)) NA_character_ else value,
      "", USE.NAMES = FALSE)),
    identical(class(frame$D), "Date"), identical(unclass(frame$D), decimal("D")),
    identical(class(frame$T), c("POSIXct", "POSIXt")), identical(attr(frame$T, "tzone"), "UTC"),
    identical(as.numeric(frame$T), decimal("T")),
    identical(format(frame$T[[3L]], "%Y-%m-%dT%H:%M:%S"), "9999-12-31T23:59:59"))

  failed_spec <- sub(host_truth[["integer_column_yaml"]],
    host_truth[["fatal_integer_column_yaml"]], spec, fixed = TRUE)
  failed_spec <- sub("output: {", host_truth[["unreached_row_count_yaml"]], failed_spec, fixed = TRUE)
  failed_spec <- sub("columns: [ID, VALUE]}", host_truth[["report_output_yaml"]], failed_spec, fixed = TRUE)
  write("spec.yaml", failed_spec); write("input.csv", host_truth[["failed_input_csv"]])
  # The file the failed run's issue is located in, as a location names it.
  failed_file <- normalizePath(path("spec.yaml"), winslash = "/", mustWork = TRUE)
  for (name in c("output.csv", "checks.csv")) write(name, "sentinel")
  failed <- yamaa_spec(path("spec.yaml"))$run(); issues <- check_issues(failed$issues)
  report <- failed$report
  checks <- report[report$KIND == "check", , drop = FALSE]
  stopifnot(identical(issues$phase, "verification"), identical(issues$condition, "range_failed"),
    identical(issues$rule, "CHECK-7"),
    identical(issues$context, '{"column":"VALUE","failure_count":1,"keys":[{"ID":"b"}],"verification_id":"fatal"}'),
    is.null(failed$output),
    identical(names(report), fixture_strings(host_truth[["report_columns"]])),
    identical(report$KIND, c("run", "input", "rows", "check", "check")),
    identical(checks$ID, c("warn", "fatal")), identical(checks$OUTCOME, c("violated", "violated")),
    identical(checks$COUNT, c(1, 1)), identical(checks$TOTAL, c(3, 3)),
    identical(checks$RULE, c("CHECK-7", "CHECK-7")))
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  # Every save rewrites the report, then raises: a deleted report comes back
  # unchanged and the stale output is never touched.
  for (attempt in 1:2) {
    error <- refusal(failed$save())
    stopifnot(identical(conditionMessage(error), error_text("failed_run_save", file = failed_file)),
      identical(error$issues, issues), identical(failed$issues, issues), identical(failed$report, report),
      identical(rawfile("output.csv"), charToRaw("sentinel")),
      identical(rawfile("checks.csv"), charToRaw(host_truth[["failed_report_csv"]])))
    unlink(path("checks.csv"))
  }
  rm(failed); invisible(gc())
  stopifnot(identical(checks$ID, c("warn", "fatal")))
  cat("public results, plain values, snapshots, failed saves and run reports passed\n")
}
run_contract()

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

qualify_callbacks <- function() {
  root <- tempfile("yamaa-public-callbacks-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  writeBin(charToRaw(host_truth[["specification_yaml"]]), path("spec.yaml"))
  writeBin(charToRaw(host_truth[["r_input_csv"]]), path("input.csv"))
  beyond <- "is beyond 2^53 and cannot be held exactly by an R double"
  original <- file.path(projects, "negative-function-contract")
  stopifnot(file.copy(original, root, recursive = TRUE))
  case <- path("negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  source <- readBin(entry, "raw", n = file.info(entry)$size)
  writeBin(charToRaw(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    rawToChar(source), fixed = TRUE)), entry)
  project <- yamaa_project(file.path(case, "project.yaml"))
  entries <- list(spec = function() project$spec("TEST")$run(), project = function() project$run())
  # Each activation stage returns the original interrupt object itself.
  for (stage in c("lock", "binding", "conformance", "derivation")) {
    for (call in entries) {
      interrupted <- structure(list(message = host_truth[["interrupt_message"]], call = NULL, token = new.env()),
        class = c("public_contract_interrupt", "interrupt", "condition"))
      invoked <- 0L
      verify <- function(...) {
        if (stage == "lock") return(list(3L, interrupted))
        capabilities$verify(...)
      }
      resolve <- function(call, parameters) {
        if (stage == "binding") return(list(3L, interrupted))
        resolved <- capabilities$resolve(call, parameters)
        if (resolved[[1L]] != 0L) return(resolved)
        invoke <- resolved[[2L]]
        list(0L, function(arguments) {
          invoked <<- invoked + 1L
          if (stage == "conformance" || (stage == "derivation" && invoked > 2L)) return(list(3L, interrupted))
          invoke(arguments)
        })
      }
      seen <- with_capabilities(verify, resolve, tryCatch(call(), interrupt = identity))
      expected <- if (stage == "conformance") 1L else if (stage == "derivation") 3L else 0L
      stopifnot(identical(seen, interrupted), identical(invoked, expected), !file.exists(file.path(case, "test.csv")))
    }
  }
  long_condition <- structure(list(message = paste0(strrep("\u00e9\n", 30000L), "complete failure"), call = NULL),
    class = c("public_contract_error", "error", "condition"))
  invoked <- 0L
  resolve_long <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      invoked <<- invoked + 1L
      if (invoked > 2L) return(list(1L, long_condition))
      invoke(arguments)
    })
  }
  long_failed <- with_capabilities(capabilities$verify, resolve_long, project$spec("TEST")$run())
  stopifnot(identical(invoked, 3L), is.null(long_failed$output),
    identical(check_issues(long_failed$issues)$condition, "function_call_failed"))
  context <- jsonlite::fromJSON(long_failed$issues$context[[1L]], simplifyVector = FALSE)
  stopifnot(identical(context$host_error, "public_contract_error"),
    identical(context$host_message, long_condition$message), is.null(context$host_details_truncated))
  refusal(long_failed$save())
  # Public operations reenter from inside a project function callback.
  calls <- list()
  resolve_reentrant <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      checked <- yamaa_spec(path("spec.yaml"))$check()
      nested <- yamaa_spec(path("spec.yaml"))$run()
      stopifnot(nrow(check_issues(checked$issues)) == 0L, nrow(check_issues(nested$issues)) == 0L)
      calls[[length(calls) + 1L]] <<- nested
      invoke(arguments)
    })
  }
  outer <- with_capabilities(capabilities$verify, resolve_reentrant, project$spec("TEST")$run())
  stopifnot(nrow(check_issues(outer$issues)) == 0L, length(calls) >= 3L)
  # A condition whose details cannot be rendered stays an opaque host failure.
  original_condition <- structure(list(message = "held output formatter failure", call = NULL),
    class = c("public_contract_error", "error", "condition"))
  writeBin(charToRaw("ID,VALUE\n01,1.234\n"), file.path(case, "input", "source.csv"))
  resolve_failed <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L, function(arguments) {
      if (identical(arguments[[1L]], 1.234)) return(list(1L, original_condition))
      invoke(arguments)
    })
  }
  failed <- with_binding(".project_condition_details", function(condition) list(),
    with_capabilities(capabilities$verify, resolve_failed, project$spec("TEST")$run()))
  measured <- yamaa:::.run_observations(failed, include_measurements = TRUE)
  stopifnot(is.character(measured), length(measured) == 1L, is.null(failed$output),
    grepl('"code":"opaque_host"', failed$issues$context, fixed = TRUE),
    grepl('"outcome":"failed","phase":"output_preparation"', measured, fixed = TRUE))
  refusal(failed$save())
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  seen <- tryCatch(calls[[1L]]$output, yamaa_error = identity)
  stopifnot(inherits(seen, "yamaa_error"), grepl(beyond, conditionMessage(seen), fixed = TRUE))
  cat("public interrupts, callback failures and reentrant operations passed\n")
}
qualify_callbacks()

qualify_project_run_members <- function() {
  root <- tempfile("yamaa-project-run-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  original <- file.path(projects, "negative-function-contract")
  stopifnot(file.copy(original, root, recursive = TRUE))
  case <- file.path(root, "negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  writeLines(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    readLines(entry), fixed = TRUE), entry)
  project <- yamaa_project(file.path(case, "project.yaml"))
  runs <- project$run()
  stopifnot(identical(runs$names, "TEST"), nrow(check_issues(runs$issues)) == 0L,
    # The project names what a run of every section builds, read without data
    # (D45).
    identical(project$names, runs$names))
  member <- runs[["TEST"]]
  single <- project$spec("TEST")$run()
  stopifnot(identical(members(member), expected_members("member_run")),
    named(member, "member_run"),
    # A project run names its specifications, and counts them.
    identical(names(runs), runs$names), identical(length(runs), length(runs$names)),
    nrow(check_issues(member$issues)) == 0L, identical(member$output, single$output),
    identical(member$report, single$report),
    # The project report stacks its members' reports in run order.
    identical(runs$report, member$report))
  locked(member, c(fixture_strings(host_truth[["read_only_views"]][["member_run"]]), "save"))
  error <- refusal(runs[["ADSL"]])
  stopifnot(identical(conditionMessage(error), error_text("unknown_member")),
    unknown_member(error, '["TEST"]'))
  # A section the project does not declare refuses the run (PROJECT-14).
  stopifnot(unknown_section(refusal(project$run("sdtm")), "sdtm", '["adam"]'))
  # A declared section that lists no specification builds nothing, and a run or
  # a submission that built nothing has nothing to save.
  listing <- file.path(case, "project.yaml")
  cat("sdtm:\n  specs: []\n", file = listing, append = TRUE)
  error <- refusal(project$run("sdtm")$save())
  stopifnot(identical(conditionMessage(error), error_text("empty_project_run_save", listing)),
    nothing_to_save(error, "PROJECT-12"))
  submission <- project$submission()
  error <- refusal(submission$save())
  stopifnot(identical(conditionMessage(error), error_text("empty_submission_save", listing)),
    nothing_to_save(error, "PROJECT-13"),
    # Without a creation time the submission reads the clock once and reports
    # the time it gave every section (D129).
    grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$", submission$created),
    identical(project$names, "TEST"))
  empty <- project$run("sdtm")
  stopifnot(identical(names(empty), character()), identical(length(empty), 0L))
  stopifnot(identical(project$run("adam")$names, "TEST"), identical(project$run("sdtm")$names, character()),
    !file.exists(file.path(case, "test.csv")), returns_paths(runs$save()),
    identical(readBin(file.path(case, "test.csv"), "raw", 100L), charToRaw("ID,RESULT\n01,1.5\n")))
  cat("project run members hold each run and only the project run saves\n")
}
qualify_project_run_members()

# Save, delete what was written, save again: the same held bytes return,
# with every source already gone.
restored <- function(directory, names, save) {
  stopifnot(returns_paths(save()))
  held <- lapply(file.path(directory, names), function(path) readBin(path, "raw", file.info(path)$size))
  unlink(file.path(directory, names))
  stopifnot(returns_paths(save()))
  for (index in seq_along(names)) {
    path <- file.path(directory, names[[index]])
    stopifnot(identical(readBin(path, "raw", file.info(path)$size), held[[index]]))
  }
  setNames(held, names)
}
qualify_every_save_rewrites <- function() {
  root <- tempfile("yamaa-every-save-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  writeBin(charToRaw(sub("columns: [ID, VALUE]}", "columns: [ID, VALUE], report: report.csv}",
    host_truth[["specification_yaml"]], fixed = TRUE)), file.path(root, "spec.yaml"))
  writeBin(charToRaw(host_truth[["r_input_csv"]]), file.path(root, "input.csv"))
  run <- yamaa_spec(file.path(root, "spec.yaml"))$run()
  stopifnot(nrow(check_issues(run$issues)) == 0L)
  unlink(file.path(root, c("spec.yaml", "input.csv")))
  held <- restored(root, c("output.csv", "report.csv"), run$save)
  stopifnot(identical(held[["output.csv"]], charToRaw(host_truth[["r_input_csv"]])))
  original <- file.path(projects, "negative-function-contract")
  stopifnot(file.copy(original, root, recursive = TRUE))
  case <- file.path(root, "negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  writeLines(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    readLines(entry), fixed = TRUE), entry)
  runs <- yamaa_project(file.path(case, "project.yaml"))$run()
  stopifnot(nrow(check_issues(runs$issues)) == 0L)
  unlink(file.path(case, "input"), recursive = TRUE); unlink(entry)
  held <- restored(case, "test.csv", runs$save)
  stopifnot(identical(held[["test.csv"]], charToRaw("ID,RESULT\n01,1.5\n")))
  fixture <- file.path(define_fixtures, "sdtm-dm-metadata")
  directory <- file.path(root, "submission"); dir.create(directory)
  stopifnot(file.copy(file.path(fixture, c("project.yaml", "spec.yaml")), directory),
    file.copy(file.path(fixture, "expected", "dm.csv"), directory))
  created <- read_host_fixture(host_fixture_root, "public-define")[["created"]]
  submission <- yamaa_project(file.path(directory, "project.yaml"))$submission("sdtm", created = created)
  stopifnot(nrow(check_issues(submission$issues)) == 0L)
  unlink(file.path(directory, c("project.yaml", "spec.yaml", "dm.csv")))
  held <- restored(directory, c("define.xml", "dm.json"), submission$save)
  for (name in names(held)) {
    path <- file.path(fixture, "expected", name)
    stopifnot(identical(held[[name]], readBin(path, "raw", file.info(path)$size)))
  }
  cat("every save rewrites what its run, project run or submission holds\n")
}
qualify_every_save_rewrites()

qualify_public_large_text_callback <- function() {
  directory <- tempfile("public-large-text-callback-"); dir.create(directory)
  on.exit(unlink(directory, recursive = TRUE))
  lock <- charToRaw(paste0('{"R":{"Version":"', getRversion(), '"},"Packages":{"yamaa":{"Version":"',
    utils::packageVersion("yamaa"), '"}}}'))
  writeBin(lock, file.path(directory, "renv.lock"))
  writeBin(charToRaw("schema_version: '1.0'\nlock: renv.lock\nfunctions:\n  text:\n    function: base::identity\n    description: Return text unchanged.\n    params: [{name: x, type: str}]\n    returns: str\n    tests:\n      - {id: normal, covers: [normal], args: {x: short}, result: short}\n      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"),
    file.path(directory, "environment.yaml"))
  writeBin(charToRaw("schema_version: '1.0'\nenvironments: {python: environment.yaml, r: environment.yaml}\nadam: {specs: [domain.yaml]}\n"),
    file.path(directory, "project.yaml"))
  writeBin(charToRaw("schema_version: '1.0'\ndomain: TEXT\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: str}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: V, type: str, derivation: {function: {name: text, args: {x: SRC.V}}}}\noutput: {path: held.csv, columns: [ID, V]}\n"),
    file.path(directory, "domain.yaml"))
  value <- paste0(strrep("\u00e9", 524289L), "complete")
  expected <- charToRaw(paste0("ID,V\n1,", value, "\n"))
  writeBin(expected, file.path(directory, "input.csv"))
  built <- yamaa_project(file.path(directory, "project.yaml"))$spec("TEXT")$run()
  stopifnot(nrow(check_issues(built$issues)) == 0L, identical(built$output$V, value))
  unlink(file.path(directory, c("domain.yaml", "project.yaml", "environment.yaml", "renv.lock", "input.csv")))
  stopifnot(returns_paths(built$save()),
    identical(readBin(file.path(directory, "held.csv"), "raw", n = length(expected) + 1L), expected))
  cat("public callback text beyond one MiB remains owned and saves exact bytes\n")
}
qualify_public_large_text_callback()

qualify_project_refusals <- function() {
  root <- tempfile("yamaa-project-refusals-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  absent <- file.path(root, "absent-project.yaml")
  project <- yamaa_project(absent)
  error <- refusal(project$spec("TEST"))
  unread <- error$issues
  # The caller's own path, named as given at no field (FILE-7).
  stopifnot(identical(conditionMessage(error), error_text("unread_project", absent)),
    identical(unread$condition, "resource_path_missing"),
    identical(unread$spec_paths, list(character())),
    identical(jsonlite::fromJSON(unread$context, simplifyVector = FALSE), list(path = absent)),
    identical(check_issues(project$check()$issues), unread))
  error <- refusal(project$names)
  stopifnot(identical(conditionMessage(error), error_text("unread_project_names", absent)),
    identical(error$issues, unread))
  runs <- project$run()
  stopifnot(identical(runs$names, character()), identical(check_issues(runs$issues), unread),
    identical(names(runs$report), fixture_strings(host_truth[["report_columns"]])),
    nrow(runs$report) == 0L, identical(members(runs), expected_members("project_run")))
  locked(runs, c(fixture_strings(host_truth[["read_only_views"]][["project_run"]]), "save"))
  error <- refusal(runs$save())
  stopifnot(identical(conditionMessage(error), error_text("refused_project_run_save", absent)),
    identical(error$issues, unread))
  error <- refusal(runs[["ADSL"]])
  stopifnot(identical(conditionMessage(error), error_text("no_member_built")), unknown_member(error, "[]"))
  submission <- project$submission("sdtm", created = "2026-09-11T00:00:00")
  stopifnot(identical(members(submission), expected_members("submission")),
    named(submission, "submission"),
    identical(check_issues(submission$issues), unread),
    # The creation time given is kept, though nothing composed (D129).
    identical(submission$created, "2026-09-11T00:00:00"))
  locked(submission, c(fixture_strings(host_truth[["read_only_views"]][["submission"]]), "save"))
  for (attempt in 1:2) {
    error <- refusal(submission$save())
    stopifnot(identical(conditionMessage(error), error_text("refused_submission_save", absent)),
      identical(error$issues, submission$issues))
  }
  # The project file is read first: one that cannot be read reports its own
  # findings, and no section is checked.
  stopifnot(identical(check_issues(project$run("bogus")$issues), unread),
    identical(check_issues(project$submission("bogus")$issues), unread))
  # A name that is no section of the project raises (PROJECT-14).
  listing <- file.path(root, "project.yaml")
  writeLines(c("schema_version: '1.0'", "adam:", "  specs: []"), listing)
  project <- yamaa_project(listing)
  for (case in list(list(call = function() project$run("bogus"), message = "unknown_section_run"),
                    list(call = function() project$submission("bogus"), message = "unknown_section_submission"))) {
    error <- refusal(case$call())
    stopifnot(identical(conditionMessage(error), error_text(case$message)),
      unknown_section(error, "bogus", '["adam"]'))
  }
  stopifnot(identical(list.files(root), "project.yaml"))
  cat("project refusals are issues, refused saves fail and unknown sections raise\n")
}
qualify_project_refusals()

qualify_issue_geometry <- function() {
  root <- tempfile("yamaa-issue-geometry-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  # A literal beyond every 64-bit range is refused before study access; the
  # context text keeps the signed 64-bit bounds and the exact literal.
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: N, type: int, derivation: {literal: 18446744073709551616}}",
    "output: {path: output.csv, columns: [ID, N]}"), path("spec.yaml"))
  frame <- check_issues(yamaa_spec(path("spec.yaml"))$check()$issues)
  stopifnot(identical(frame$phase, "validation"), identical(frame$condition, "integer_overflow"),
    identical(frame$rule, "VALUE-5"), identical(frame$spec_paths, list("columns.N.derivation.literal")),
    identical(frame$context, '{"actual":18446744073709551616,"maximum":9223372036854775807,"minimum":-9223372036854775808}'),
    is.null(frame$source_span[[1L]]), is.null(frame$operand_route[[1L]]))
  # A division by zero fails at one node of the compute expression: the span
  # covers that node's bytes and the route leads to it from the root, so a
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
    result <- yamaa_spec(path("spec.yaml"))$run()
    frame <- check_issues(result$issues)
    span <- frame$source_span[[1L]]
    stopifnot(is.null(result$output), identical(frame$condition, "division_by_zero"),
      identical(substr(case$expression, span[[1L]] + 1, span[[2L]]), "VALUE / ZERO"),
      identical(frame$operand_route[[1L]], case$route),
      grepl('"keys":[{"ID":"a"}]', frame$context, fixed = TRUE))
  }
  cat("issue rows keep exact wide integers and distinguish recorded from absent geometry\n")
}
qualify_issue_geometry()

# Python reads the same cases, and both hosts equal this one truth. The
# context is compared as its canonical JSON text, so no JSON reader chooses a
# number type; spans are doubles, routes character, and both NULL when absent.
qualify_issue_parity <- function() {
  root <- tempfile("yamaa-issue-parity-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  for (case in read_host_fixture(host_fixture_root, "diagnostic-parity")$cases) {
    directory <- file.path(root, case$name); dir.create(directory)
    spec <- file.path(directory, "spec.yaml")
    if (!is.null(case$specification_yaml)) {
      writeBin(charToRaw(enc2utf8(case$specification_yaml)), spec)
      writeBin(fixture_bytes(case$input_csv), file.path(directory, "input.csv"))
    }
    frame <- check_issues(yamaa_spec(spec)[[case$call]]()$issues)
    expected <- case$issues
    # `{path}` stands for the specification path given, as JSON text in the
    # context.
    given <- c(message = spec, context = gsub("\\", "\\\\", spec, fixed = TRUE))
    text <- function(field) vapply(expected, function(issue)
      if (is.null(issue[[field]])) NA_character_
      else if (field %in% names(given)) gsub("{path}", given[[field]], issue[[field]], fixed = TRUE)
      else issue[[field]], "")
    number <- function(field) vapply(expected, function(issue)
      if (is.null(issue[[field]])) NA_real_ else as.double(issue[[field]]), 0)
    actual <- list(severity = frame$severity, phase = frame$phase, condition = frame$condition,
      rule = frame$rule, spec_paths = frame$spec_paths, file = frame$file, line = frame$line,
      column = frame$column, message = frame$message, context = frame$context,
      source_span = frame$source_span, operand_route = frame$operand_route)
    truth <- list(severity = text("severity"), phase = text("phase"),
      condition = text("condition"), rule = text("rule"),
      spec_paths = lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
      # A truth row names its file relative to the case's directory; a
      # location names it by its absolute path (P-ISSUE-1).
      file = vapply(expected, function(issue) if (is.null(issue$file)) NA_character_ else
        normalizePath(file.path(directory, issue$file), winslash = "/", mustWork = TRUE), ""),
      line = number("line"), column = number("column"),
      message = enc2utf8(text("message")), context = enc2utf8(text("context")),
      source_span = lapply(expected, function(issue)
        if (is.null(issue$source_span)) NULL else as.double(unlist(issue$source_span))),
      operand_route = lapply(expected, function(issue)
        if (is.null(issue$operand_route)) NULL else as.character(unlist(issue$operand_route))))
    if (!identical(actual, truth)) {
      str(actual); str(truth)
      stop("issue rows differ from the shared parity truth: ", case$name)
    }
  }
  cat("issue rows equal the shared parity truth field for field\n")
}
qualify_issue_parity()

qualify_resolve <- function() {
  root <- tempfile("yamaa-resolve-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  entry <- file.path(root, "spec.yaml")
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing/raw.csv, types: {ID: int}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: absent/output.csv, columns: [ID]}"), entry)
  spec <- yamaa_spec(entry)
  facts <- spec$resolve()
  resource <- Filter(function(value) identical(value$role, "input_data"), facts$entries[[1L]]$resources)[[1L]]
  stopifnot(is.list(facts), identical(facts$format, "yamaa.resolved-specification"),
    identical(facts$artifact_version, 1), identical(facts$entry, entry),
    identical(facts$replay, "inspection_only"), "project" %in% names(facts), is.null(facts[["project"]]),
    identical(resource$location, list(kind = "local", path = file.path(root, "missing/raw.csv"))),
    !dir.exists(file.path(root, "absent")))
  unlink(entry)
  error <- refusal(spec$resolve())
  stopifnot(identical(conditionMessage(error), error_text("resolve", entry)),
    identical(error$issues$condition, "resource_path_missing"))

  # The project supplies the function and names itself; the same file alone
  # names an unknown function. No callable is activated.
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}, {name: V, type: str, derivation: {function: {name: text, args: {x: RAW.V}}}}]",
    "output: {path: output.csv, columns: [ID, V]}"), entry)
  writeLines(c("schema_version: '1.0'", "functions:", "  text:",
    "    function: uninstalledresolvedfixture::identity", "    description: Identity.",
    "    params: [{name: x, type: str}]", "    returns: str", "    tests:",
    "      - {id: normal, covers: [normal], args: {x: a}, result: a}",
    "      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}",
    "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}"),
    file.path(root, "environment.yaml"))
  writeLines(c("schema_version: '1.0'", "environments: {python: environment.yaml, r: environment.yaml}",
    "adam: {specs: [spec.yaml]}"), file.path(root, "project.yaml"))
  refuse <- function(...) stop("unexpected activation")
  facts <- with_capabilities(refuse, refuse,
    yamaa_project(file.path(root, "project.yaml"))$spec("TEST")$resolve())
  stopifnot(identical(facts$project, file.path(root, "project.yaml")), identical(facts$entry, entry))
  error <- refusal(yamaa_spec(entry)$resolve())
  stopifnot(identical(error$issues$condition, "unknown_project_function"),
    identical(error$issues$rule, "FUNC-4"),
    identical(error$issues$spec_paths, list("columns.V.derivation.function.name")),
    identical(error$issues$context, '{"function":"text"}'))

  # A mapping key holding NUL is both text and a name in the facts, and R
  # holds neither: resolve refuses it instead of failing inside R.
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}, {name: W, type: str, derivation: {mapping: {source: RAW.V, dict: {\"L\\0\": LOW}}}}]",
    "output: {path: output.csv, columns: [ID, W]}"), entry)
  error <- refusal(yamaa_spec(entry)$resolve())
  stopifnot(grepl("containing NUL cannot be held by an R character vector",
    conditionMessage(error), fixed = TRUE))
  cat("resolve returns plain facts, names the project and refuses unreadable specifications\n")
}
qualify_resolve()

qualify_resolved_schema_links <- function() {
  root <- tempfile("yamaa-resolved-links-")
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
    identifier <- if (identical(wrapper, "A")) 17 else 29
    writeLines(c("ID", identifier), file.path(study, "input/raw.csv"))
    writeLines(c("schema_version: '1.0'", "parent: ../shared/consumer.yaml"), file.path(study, "spec.yaml"))
    writeLines(c("schema_version: '1.0'", "parent: ../shared/producer.yaml"), file.path(study, "producer.yaml"))
    spec <- yamaa_spec(file.path(study, "spec.yaml"))
    entries <- spec$resolve()$entries
    # A linked producer is type metadata, not a resolved entry.
    stopifnot(length(entries) == 1L, is.null(entries[[1L]]$producers))
    schema <- Filter(function(value) identical(value$role, "producer_schema"), entries[[1L]]$resources)[[1L]]
    stopifnot(identical(schema$location$path, file.path(study, "producer.yaml")))
    source <- Filter(function(value) identical(value$role, "input_data"), entries[[1L]]$resources)[[1L]]
    stopifnot(identical(source$location$path, file.path(study, "output/produced.csv")))
    # Callers build and save each study's producer first.
    dir.create(file.path(study, "output"))
    stopifnot(returns_paths(yamaa_spec(file.path(study, "producer.yaml"))$run()$save()))
    result <- spec$run()
    stopifnot(nrow(result$issues) == 0L, identical(result$output$ID, identifier),
      !file.exists(file.path(study, "output/final.csv")))
  }
  cat("resolved schema links agree with ordered direct runs for both studies\n")
}
qualify_resolved_schema_links()

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
  writeLines(c("schema_version: '1.0'", "parent: base.yaml"), child)
  origins <- function(path) yamaa_spec(path)$resolve()$entries[[1L]]$origins
  direct <- origins(base)
  inherited <- origins(child)
  pointer <- "/columns/0/derivation/value/mapping/case_sensitive"
  stopifnot(identical(direct[[pointer]]$source_kind, "schema_default"),
    identical(inherited[[pointer]], direct[[pointer]]), isTRUE(inherited[["/input/RAW"]]$generated))
  writeLines(c("schema_version: '1.0'", "domain: TEST", "keys: [ID]", "input: {RAW: missing.csv}",
    "windows: {W: {order_by: [RAW.ID]}}", "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: result.csv, columns: [ID]}"), base)
  writeLines(c("schema_version: '1.0'", "parent: base.yaml",
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
