# Installed public API contracts; inputs and exact truth are independently authored.
library(yamaa)
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
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-contract")
messages <- host_truth[["messages"]]
# D39's error text the fixture authors under `key`, naming the path given
# and `file`, the file its first issue is located in, as its location names
# it (printed_file()).
error_text <- function(key, path = NULL, file = NULL) {
  text <- messages[[key]]
  if (!is.null(file)) text <- sub("{file}", file, text, fixed = TRUE)
  if (is.null(path)) text else gsub("{path}", path, text, fixed = TRUE)
}
namespace <- asNamespace("yamaa")

# A final release's package and engine carry one version. An R version cannot
# mark a release candidate, so a candidate's package keeps the final version
# while the engine names the candidate in its Cargo spelling (#3030 D53).
expect_identical(as.character(packageVersion("yamaa")), "0.6.2", "the package version")
expect_identical(yamaa:::.core_version(), "0.6.2", "the engine's version")
expect_identical(sort(getNamespaceExports("yamaa")),
  c("yamaa_conditions", "yamaa_info", "yamaa_lint", "yamaa_project", "yamaa_read_odm",
    "yamaa_schema", "yamaa_selftest", "yamaa_spec"),
  "the exported functions")
expect_identical(getNamespaceInfo("yamaa", "S3methods")[, 3L],
    c("names.yamaa_check", "length.yamaa_check", "print.yamaa_check", "format.yamaa_check",
      "names.yamaa_run", "length.yamaa_run", "print.yamaa_run", "format.yamaa_run",
      "names.yamaa_member_run", "length.yamaa_member_run", "print.yamaa_member_run",
      "format.yamaa_member_run", "names.yamaa_project_run", "length.yamaa_project_run",
      "[[.yamaa_project_run", "print.yamaa_project_run", "format.yamaa_project_run",
      "names.yamaa_submission", "length.yamaa_submission", "print.yamaa_submission",
      "format.yamaa_submission", "print.yamaa_spec", "format.yamaa_spec", "print.yamaa_project",
      "format.yamaa_project", "names.yamaa_selftest", "length.yamaa_selftest",
      "print.yamaa_selftest", "format.yamaa_selftest"),
  "the registered S3 methods")
for (name in c("yamaa_domain", "yamaa_check", "yamaa_define", "yamaa_resolve_specification",
  "yamaa_read_resolved_specification", "yamaa_resource_resolver", "yamaa_memory_resolver",
  "yamaa_synthetic_remote_resolver", "yamaa_resource_result", "yamaa_resource_failure",
  "yamaa_int64", "yamaa_utf8")) {
  if (exists(name, envir = namespace, inherits = FALSE)) stop("a retired interface remains: ", name)
}

# The one findings view: the twelve columns of D28, context as canonical JSON
# text, the geometry columns lists holding NULL where nothing was recorded,
# and a location in all three of file, line and column or in none
# (P-ISSUE-1).
check_issues <- function(frame) {
  stopifnot("the issues are a data frame" = is.data.frame(frame))
  expect_identical(names(frame), fixture_strings(host_truth[["issue_columns"]]), "the issue columns")
  stopifnot(
    "the text columns are character" =
      all(vapply(frame[c("severity", "phase", "condition", "rule", "file", "message", "context")],
        is.character, logical(1))),
    "line and column are double" = all(vapply(frame[c("line", "column")], is.double, logical(1))),
    "every issue is an error" = all(frame$severity == "error"),
    "every issue has a message" = all(nzchar(frame$message)),
    "a location has a file and a line, or neither" = identical(is.na(frame$file), is.na(frame$line)),
    "a location has a file and a column, or neither" = identical(is.na(frame$file), is.na(frame$column)),
    "spec paths, spans and routes are list columns" =
      all(vapply(frame[c("spec_paths", "source_span", "operand_route")], is.list, logical(1))),
    "spec paths are character" = all(vapply(frame$spec_paths, is.character, logical(1))),
    "every context is a JSON object" = all(startsWith(frame$context, "{")),
    "a span is NULL or two ordered doubles" =
      all(vapply(frame$source_span, function(span) is.null(span) || (is.double(span) &&
        length(span) == 2L && span[[1L]] >= 0 && span[[1L]] <= span[[2L]]), logical(1))),
    "a route is NULL or character" =
      all(vapply(frame$operand_route, function(route) is.null(route) || is.character(route), logical(1))))
  frame
}
# A failed operation signals yamaa_error carrying the issues that explain it.
refusal <- function(code) {
  seen <- tryCatch({ force(code); NULL }, yamaa_error = identity)
  stopifnot("the operation signals yamaa_error" = inherits(seen, "yamaa_error"),
    "yamaa_error is an error" = inherits(seen, "error"),
    "yamaa_error has no call" = is.null(conditionCall(seen)))
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
# Whether a refusal carries exactly one issue equal to a shared truth row
# that, like every project refusal, has no location and no geometry.
one_truth_row <- function(error, row) {
  issue <- error$issues
  nrow(issue) == 1L && is.null(row$file) && is.null(row$line) && is.null(row$column) &&
    is.null(row$source_span) && is.null(row$operand_route) &&
    identical(list(issue$severity, issue$phase, issue$condition, issue$rule, issue$spec_paths,
      issue$file, issue$line, issue$column, issue$message, issue$context, issue$source_span,
      issue$operand_route),
      list(row$severity, row$phase, row$condition, row$rule,
        list(as.character(unlist(row$spec_paths))), NA_character_, NA_real_, NA_real_,
        row$message, row$context, list(NULL), list(NULL)))
}
# The one issue of a call that selects a section the project does not
# declare: the shared truth under `key`, which Python compares too.
unknown_section <- function(error, key)
  one_truth_row(error, host_truth[["unknown_section_issues"]][[key]])
# The one issue of a save with nothing to write: the shared truth under `key`,
# which Python compares too.
nothing_to_save <- function(error, key)
  one_truth_row(error, host_truth[["nothing_to_save_issues"]][[key]])
# Argument mistakes are ordinary errors, never yamaa_error.
argument_failure <- function(code) {
  seen <- tryCatch({ force(code); NULL }, error = identity)
  stopifnot("an argument mistake is an error" = inherits(seen, "error"),
    "an argument mistake is not yamaa_error" = !inherits(seen, "yamaa_error"),
    "an argument mistake has a message" = is.character(conditionMessage(seen)))
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
    if (!inherits(tryCatch({ assign(name, NULL, envir = object); NULL }, error = identity), "error"))
      stop("a locked member could be assigned: ", name)
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
  stopifnot("read_odm's archive member defaults to NULL" = is.null(options$archive_member),
    "a spec check's parent defaults to NULL" = is.null(formals(spec$check)$parent),
    "a project run's section defaults to NULL" = is.null(formals(project$run)$section),
    "a submission's section defaults to NULL" = is.null(formals(project$submission)$section),
    "a submission's creation time defaults to NULL" = is.null(formals(project$submission)$created),
    "a specification's run does not stream by default" = identical(formals(spec$run)$stream, FALSE))
  for (name in names(limits)) {
    expect_identical(options[[name]], as.numeric(limits[[name]]), paste("read_odm's default", name))
  }
  expect_identical(members(spec), expected_members("spec"), "a spec's members")
  expect_identical(members(project), expected_members("project"), "a project's members")
  expect_identical(members(spec$check()), expected_members("check"), "a check's members")
  stopifnot("a check names its members" = named(spec$check(), "check"))
  run <- spec$run()
  expect_identical(members(run), expected_members("run"), "a run's members")
  stopifnot("a run names its members" = named(run, "run"),
    "a run's status is character" = is.character(run$status),
    "[[ reaches a run's status" = identical(run[["status"]], run$status))
  expect_identical(arguments_of(run$save), expected_arguments("run.save"), "a run's save arguments")
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
  expect_identical(list.files(root), character(), "argument mistakes write nothing")
  cat("public exports, signatures, members, locked results and argument errors passed\n")
}
qualify_signatures()

# printed_file() judges "beneath" by directory, not by spelling. The study
# directory is spelled canonically, through `..`, and through a link on Unix
# or with its 8.3 short names on Windows. Whichever spelling is the working
# directory, a file below the study, spelled through any of them, prints
# relative to it. A sibling whose name starts with the study's, and the
# parent's own file, print whole.
qualify_printed_file <- function() {
  root <- tempfile("yamaa-printed-file-")
  dir.create(file.path(root, "study", "adam"), recursive = TRUE)
  dir.create(file.path(root, "studyx"))
  home <- getwd()
  on.exit({ setwd(home); unlink(root, recursive = TRUE) })
  root <- normalizePath(root, winslash = "/")
  study <- file.path(root, "study")
  spellings <- c(study, file.path(study, "adam", ".."))
  if (.Platform$OS.type == "windows") {
    short <- utils::shortPathName(normalizePath(study, winslash = "\\"))
    spellings <- c(spellings, gsub("\\", "/", short, fixed = TRUE))
  } else if (file.symlink(study, file.path(root, "link"))) {
    spellings <- c(spellings, file.path(root, "link"))
  }
  outside <- file.path(root, c("studyx/dm.yaml", "dm.yaml"))
  for (working in spellings) {
    setwd(working)
    actual <- vapply(c(file.path(spellings, "adam/adae.yaml"), file.path(spellings, "dm.yaml"), outside),
      printed_file, "", USE.NAMES = FALSE)
    expected <- c(rep(c("adam/adae.yaml", "dm.yaml"), each = length(spellings)), outside)
    if (!identical(actual, expected)) {
      str(actual); str(expected)
      stop("printed_file() does not judge files beneath the working directory by directory: ", working)
    }
  }
  cat("an issue's file prints relative to the working directory however either is spelled\n")
}
qualify_printed_file()

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
    stopifnot("the run has no issues" = nrow(check_issues(result$issues)) == 0L,
      "a run is an environment" = is.environment(result),
      "the frame refuses integers beyond 2^53" = refuses_output(result, beyond),
      "the report is a data frame" = is.data.frame(report))
    expect_identical(names(report), fixture_strings(host_truth[["report_columns"]]), "the report columns")
    expect_identical(report$KIND, c("run", "input", "rows", "column", "column"), "the report's rows")
    expect_identical(report$OUTCOME[[1L]], "succeeded", "the run row's outcome")
    expect_identical(report$COUNT[[1L]], 5, "the run row counts five rows")
    locked(result, fixture_strings(host_truth[["read_only_views"]][["run"]]))
  }
  stopifnot("nothing is written before save" = !file.exists(path("output.csv")))
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  # save() returns the one path it wrote, invisibly (D38).
  saved <- withVisible(first$save())
  stopifnot("save returns invisibly" = !saved$visible)
  expect_identical(saved$value, normalizePath(path("output.csv"), winslash = "/"), "save returns the path it wrote")
  expect_identical(rawfile("output.csv"), charToRaw(input), "the saved CSV keeps the input bytes")
  rm(first); invisible(gc())
  stopifnot("the second run still refuses its frame" = refuses_output(second, beyond),
    "the second run saves" = returns_paths(second$save()))

  absent <- yamaa_spec(path("absent.yaml"))
  checked <- check_issues(absent$check()$issues)
  run <- absent$run()
  # The caller's own path, named as given at no field (FILE-7).
  expect_identical(as.list(checked[c("condition", "rule", "spec_paths")]),
    list(condition = "resource_path_missing", rule = "FILE-7", spec_paths = list(character())),
    "an absent specification's one issue")
  expect_identical(jsonlite::fromJSON(checked$context, simplifyVector = FALSE), list(path = path("absent.yaml")),
    "an absent specification's issue names the path given")
  stopifnot("a run of an absent specification has no output" = is.null(run$output),
    "a run of an absent specification has no report" = is.null(run$report))
  expect_identical(run$issues, checked, "the run's issues are the check's")
  for (attempt in 1:2) {
    error <- refusal(run$save())
    label <- sprintf("save attempt %d of an absent specification", attempt)
    # A run whose file was not read names the path as given.
    expect_identical(conditionMessage(error), error_text("missing_run_save", path("absent.yaml")),
      paste(label, "gives D39's text"))
    expect_identical(error$issues, checked, paste(label, "carries the run's issues"))
    expect_identical(run$issues, checked, paste(label, "leaves the run's issues"))
    expect_identical(error$paths, character(), paste(label, "wrote nothing"))
  }
  error <- refusal(absent$resolve())
  expect_identical(conditionMessage(error), error_text("resolve", path("absent.yaml")),
    "resolving an absent specification gives D39's text")
  expect_identical(error$issues, checked, "resolving an absent specification carries the check's issues")

  text_spec <- sub("VALUE, type: int", "VALUE, type: str", spec, fixed = TRUE)
  text_spec <- sub("output: {", host_truth[["empty_text_column"]], text_spec, fixed = TRUE)
  text_spec <- sub("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]", text_spec, fixed = TRUE)
  write("spec.yaml", text_spec)
  write("input.csv", c(charToRaw("ID,VALUE\na,a"), as.raw(0L), charToRaw("b\nb,"),
    as.raw(c(0xc3, 0xa9)), charToRaw("\nc,\n")))
  text <- yamaa_spec(path("spec.yaml"))$run()
  stopifnot("the text run has no issues" = nrow(check_issues(text$issues)) == 0L)
  # Text holding NUL has no R character value either: the frame refuses it and
  # save still publishes the held bytes.
  stopifnot("the frame refuses text holding NUL" =
      refuses_output(text, "text containing NUL cannot be held by an R character vector"),
    "the text run saves" = returns_paths(text$save()))
  expected <- c(charToRaw('ID,VALUE,EMPTY\na,a'), as.raw(0L), charToRaw('b,""\nb,'),
    as.raw(c(0xc3, 0xa9)), charToRaw(',""\nc,,""\n'))
  expect_identical(rawfile("output.csv"), expected, "the saved text keeps NUL, UTF-8 and empty strings")

  write("spec.yaml", host_truth[["typed_specification_yaml"]])
  write("input.csv", host_truth[["typed_input_csv"]])
  typed <- yamaa_spec(path("spec.yaml"))$run()
  stopifnot("the typed run has no issues" = nrow(check_issues(typed$issues)) == 0L)
  frame <- typed$output
  values <- host_truth[["typed_values"]]
  decimal <- function(name) vapply(values[[name]], function(value)
    if (is.null(value)) NA_real_ else as.double(value), double(1), USE.NAMES = FALSE)
  stopifnot("the typed output is a data frame" = is.data.frame(frame))
  expect_identical(names(frame), names(values), "the typed columns")
  for (name in c("ID", "I", "F")) expect_identical(frame[[name]], decimal(name), paste("typed column", name))
  expect_identical(1 / frame$F[[1L]], -Inf, "the first float is negative zero")
  expect_identical(frame$S, vapply(values$S, function(value) if (is.null(value)) NA_character_ else value,
    "", USE.NAMES = FALSE), "typed column S")
  expect_identical(class(frame$D), "Date", "a date column is Date")
  expect_identical(unclass(frame$D), decimal("D"), "typed column D counts days")
  expect_identical(class(frame$T), c("POSIXct", "POSIXt"), "a datetime column is POSIXct")
  expect_identical(attr(frame$T, "tzone"), "UTC", "a datetime column is in UTC")
  expect_identical(as.numeric(frame$T), decimal("T"), "typed column T counts seconds")
  expect_identical(format(frame$T[[3L]], "%Y-%m-%dT%H:%M:%S"), "9999-12-31T23:59:59",
    "the last representable datetime")

  failed_spec <- sub(host_truth[["integer_column_yaml"]],
    host_truth[["fatal_integer_column_yaml"]], spec, fixed = TRUE)
  failed_spec <- sub("output: {", host_truth[["unreached_row_count_yaml"]], failed_spec, fixed = TRUE)
  failed_spec <- sub("columns: [ID, VALUE]}", host_truth[["report_output_yaml"]], failed_spec, fixed = TRUE)
  write("spec.yaml", failed_spec); write("input.csv", host_truth[["failed_input_csv"]])
  for (name in c("output.csv", "checks.csv")) write(name, "sentinel")
  failed <- yamaa_spec(path("spec.yaml"))$run(); issues <- check_issues(failed$issues)
  report <- failed$report
  checks <- report[report$KIND == "check", , drop = FALSE]
  expect_identical(as.list(issues[c("phase", "condition", "rule", "context")]),
    list(phase = "verification", condition = "range_failed", rule = "CHECK-7",
      context = '{"column":"VALUE","failure_count":1,"keys":[{"ID":"b"}],"verification_id":"fatal"}'),
    "the failed run's one issue")
  # The issue names the specification by its absolute, forward-slash path.
  # Compare file identity because R and the engine may spell it differently.
  stopifnot("the issue's file is absolute" = grepl("^(/|[[:alpha:]]:/)", issues$file),
    "the issue's file uses forward slashes" = !grepl("\\", issues$file, fixed = TRUE))
  expect_identical(normalizePath(issues$file, winslash = "/", mustWork = TRUE),
    normalizePath(path("spec.yaml"), winslash = "/", mustWork = TRUE),
    "the issue is located in the specification")
  stopifnot("a failed run has no output" = is.null(failed$output))
  expect_identical(names(report), fixture_strings(host_truth[["report_columns"]]), "the failed run's report columns")
  expect_identical(report$KIND, c("run", "input", "rows", "check", "check"), "the failed run's report rows")
  expect_identical(as.list(checks[c("ID", "OUTCOME", "COUNT", "TOTAL", "RULE")]),
    list(ID = c("warn", "fatal"), OUTCOME = c("violated", "violated"), COUNT = c(1, 1),
      TOTAL = c(3, 3), RULE = c("CHECK-7", "CHECK-7")),
    "the failed run's check rows")
  # The file the failed run's issue is located in, as its location names it.
  failed_file <- printed_file(issues$file)
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  # Every save rewrites the report, then raises: a deleted report comes back
  # unchanged and the stale output is never touched.
  for (attempt in 1:2) {
    error <- refusal(failed$save())
    label <- sprintf("save attempt %d of the failed run", attempt)
    expect_identical(conditionMessage(error), error_text("failed_run_save", file = failed_file),
      paste(label, "gives D39's text"))
    expect_identical(error$issues, issues, paste(label, "carries the run's issues"))
    expect_identical(failed$issues, issues, paste(label, "leaves the run's issues"))
    expect_identical(failed$report, report, paste(label, "leaves the run's report"))
    expect_identical(rawfile("output.csv"), charToRaw("sentinel"), paste(label, "leaves the stale output"))
    expect_identical(rawfile("checks.csv"), charToRaw(host_truth[["failed_report_csv"]]),
      paste(label, "rewrites the report"))
    unlink(path("checks.csv"))
  }
  rm(failed); invisible(gc())
  expect_identical(checks$ID, c("warn", "fatal"), "a report view outlives its run")
  cat("public results, plain values, snapshots, failed saves and run reports passed\n")
}
run_contract()

capabilities <- get(".yamaa_locked_host_capabilities", namespace)()

qualify_callbacks <- function() {
  root <- tempfile("yamaa-public-callbacks-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  writeBin(charToRaw(host_truth[["specification_yaml"]]), path("spec.yaml"))
  writeBin(charToRaw(host_truth[["r_input_csv"]]), path("input.csv"))
  beyond <- "is beyond 2^53 and cannot be held exactly by an R double"
  original <- file.path(projects, "negative-function-contract")
  stopifnot("the project case is copied" = file.copy(original, root, recursive = TRUE))
  case <- path("negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  source <- readBin(entry, "raw", n = file.info(entry)$size)
  writeBin(charToRaw(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    rawToChar(source), fixed = TRUE)), entry)
  project <- yamaa_project(file.path(case, "project.yaml"))
  entries <- list(spec = function() project$spec("TEST")$run(), project = function() project$run())
  # Each activation stage returns the original interrupt object itself.
  for (stage in c("lock", "binding", "conformance", "derivation")) {
    for (entry_name in names(entries)) {
      call <- entries[[entry_name]]
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
      label <- sprintf("an interrupt at the %s stage of a %s run", stage, entry_name)
      expect_identical(seen, interrupted, paste(label, "returns the original interrupt"))
      expect_identical(invoked, expected, paste(label, "follows this many invocations"))
      stopifnot("an interrupted run writes nothing" = !file.exists(file.path(case, "test.csv")))
    }
  }
  # Windows cannot send SIGINT to the current process with tools::pskill.
  if (.Platform$OS.type == "unix") {
    for (entry_name in names(entries)) {
      invoked <- 0L
      # The stand-in replies through a closed envelope, as the installed
      # capability does: R takes the pending interrupt at its next check, and
      # one taken outside an envelope would end the callback as a failure.
      resolve_signal <- function(call, parameters) {
        resolved <- capabilities$resolve(call, parameters)
        if (resolved[[1L]] != 0L) return(resolved)
        invoke <- resolved[[2L]]
        list(0L, function(arguments) tryCatch({
          invoked <<- invoked + 1L
          if (invoked == host_truth[["signal_interrupt_calls"]]) tools::pskill(Sys.getpid(), signal = 2L)
          invoke(arguments)
        }, interrupt = function(condition) list(3L, condition)))
      }
      seen <- with_capabilities(capabilities$verify, resolve_signal,
        tryCatch(entries[[entry_name]](), interrupt = identity))
      actual <- list(interrupt = inherits(seen, "interrupt"), calls = invoked,
        published = file.exists(file.path(case, "test.csv")))
      expected <- list(interrupt = TRUE, calls = as.integer(host_truth[["signal_interrupt_calls"]]), published = FALSE)
      expect_identical(actual, expected, paste("a SIGINT in a callback of a", entry_name, "run stops it without publishing"))
    }
  } else {
    cat("SKIP: in-process SIGINT requires tools::pskill on Unix\n")
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
  expect_identical(invoked, 3L, "the long failure ends the third invocation")
  stopifnot("a failed callback leaves no output" = is.null(long_failed$output))
  expect_identical(check_issues(long_failed$issues)$condition, "function_call_failed",
    "a failed callback's issue")
  context <- jsonlite::fromJSON(long_failed$issues$context[[1L]], simplifyVector = FALSE)
  expect_identical(context$host_error, "public_contract_error", "the host error's class")
  expect_identical(context$host_message, long_condition$message, "the host message is complete")
  stopifnot("the host details are not truncated" = is.null(context$host_details_truncated))
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
      stopifnot("a reentrant check has no issues" = nrow(check_issues(checked$issues)) == 0L,
        "a reentrant run has no issues" = nrow(check_issues(nested$issues)) == 0L)
      calls[[length(calls) + 1L]] <<- nested
      invoke(arguments)
    })
  }
  outer <- with_capabilities(capabilities$verify, resolve_reentrant, project$spec("TEST")$run())
  stopifnot("the outer run has no issues" = nrow(check_issues(outer$issues)) == 0L,
    "every callback reentered" = length(calls) >= 3L)
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
  stopifnot("the measured observations are one string" = is.character(measured) && length(measured) == 1L,
    "an opaque host failure leaves no output" = is.null(failed$output),
    "an unrenderable condition is an opaque host failure" =
      grepl('"code":"opaque_host"', failed$issues$context, fixed = TRUE),
    "output preparation failed" =
      grepl('"outcome":"failed","phase":"output_preparation"', measured, fixed = TRUE))
  refusal(failed$save())
  unlink(path(c("spec.yaml", "input.csv"))); invisible(gc())
  seen <- tryCatch(calls[[1L]]$output, yamaa_error = identity)
  stopifnot("a nested run outlives its sources and still refuses its frame" =
    inherits(seen, "yamaa_error") && grepl(beyond, conditionMessage(seen), fixed = TRUE))
  cat("public interrupts, callback failures and reentrant operations passed\n")
}
qualify_callbacks()

qualify_project_run_members <- function() {
  root <- tempfile("yamaa-project-run-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  original <- file.path(projects, "negative-function-contract")
  stopifnot("the project case is copied" = file.copy(original, root, recursive = TRUE))
  case <- file.path(root, "negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  writeLines(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    readLines(entry), fixed = TRUE), entry)
  project <- yamaa_project(file.path(case, "project.yaml"))
  runs <- project$run()
  expect_identical(runs$names, "TEST", "the project run builds TEST")
  stopifnot("the project run has no issues" = nrow(check_issues(runs$issues)) == 0L)
  # The project names what a run of every section builds, read without data
  # (D45).
  expect_identical(project$names, runs$names, "the project names what its run builds")
  member <- runs[["TEST"]]
  single <- project$spec("TEST")$run()
  expect_identical(members(member), expected_members("member_run"), "a member run's members")
  stopifnot("a member run names its members" = named(member, "member_run"))
  # A project run names its specifications, and counts them.
  expect_identical(names(runs), runs$names, "a project run's names")
  expect_identical(length(runs), length(runs$names), "a project run's length")
  stopifnot("the member has no issues" = nrow(check_issues(member$issues)) == 0L)
  expect_identical(member$output, single$output, "the member's output is a single run's")
  expect_identical(member$report, single$report, "the member's report is a single run's")
  # The project report stacks its members' reports in run order.
  expect_identical(runs$report, member$report, "the project report stacks its members'")
  locked(member, c(fixture_strings(host_truth[["read_only_views"]][["member_run"]]), "save"))
  error <- refusal(runs[["ADSL"]])
  expect_identical(conditionMessage(error), error_text("unknown_member"), "an unknown member's error text")
  stopifnot("an unknown member's one issue" = unknown_member(error, '["TEST"]'))
  # A section the project does not declare refuses the run (PROJECT-14).
  stopifnot("an undeclared section refuses the run" =
    unknown_section(refusal(project$run("sdtm")), "sdtm"))
  # A declared section that lists no specification builds nothing, and a run or
  # a submission that built nothing has nothing to save.
  listing <- file.path(case, "project.yaml")
  cat("sdtm:\n  specs: []\n", file = listing, append = TRUE)
  error <- refusal(project$run("sdtm")$save())
  expect_identical(conditionMessage(error), error_text("empty_project_run_save", listing),
    "an empty project run's save text")
  stopifnot("an empty project run has nothing to save" = nothing_to_save(error, "empty_project_run_save"))
  submission <- project$submission()
  error <- refusal(submission$save())
  expect_identical(conditionMessage(error), error_text("empty_submission_save", listing),
    "an empty submission's save text")
  stopifnot("an empty submission has nothing to save" = nothing_to_save(error, "empty_submission_save"),
    # Without a creation time the submission reads the clock once and reports
    # the time it gave every section (D129).
    "the submission reports the clock time it read" =
      grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$", submission$created))
  expect_identical(project$names, "TEST", "an empty section adds no names")
  empty <- project$run("sdtm")
  expect_identical(names(empty), character(), "an empty project run names nothing")
  expect_identical(length(empty), 0L, "an empty project run has length 0")
  expect_identical(project$run("adam")$names, "TEST", "the adam section builds TEST")
  expect_identical(project$run("sdtm")$names, character(), "the sdtm section builds nothing")
  stopifnot("nothing is written before save" = !file.exists(file.path(case, "test.csv")),
    "the project run saves" = returns_paths(runs$save()))
  expect_identical(readBin(file.path(case, "test.csv"), "raw", 100L), charToRaw("ID,RESULT\n01,1.5\n"),
    "the project run saves its member's CSV")
  cat("project run members hold each run and only the project run saves\n")
}
qualify_project_run_members()

# Save, delete what was written, save again: the same held bytes return,
# with every source already gone.
restored <- function(directory, names, save) {
  stopifnot("the first save writes" = returns_paths(save()))
  held <- lapply(file.path(directory, names), rawfile)
  unlink(file.path(directory, names))
  stopifnot("the second save writes again" = returns_paths(save()))
  for (index in seq_along(names)) {
    expect_identical(rawfile(file.path(directory, names[[index]])), held[[index]],
      paste("the second save rewrites the same", names[[index]]))
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
  stopifnot("the run has no issues" = nrow(check_issues(run$issues)) == 0L)
  unlink(file.path(root, c("spec.yaml", "input.csv")))
  held <- restored(root, c("output.csv", "report.csv"), run$save)
  expect_identical(held[["output.csv"]], charToRaw(host_truth[["r_input_csv"]]), "a run saves its held output")
  original <- file.path(projects, "negative-function-contract")
  stopifnot("the project case is copied" = file.copy(original, root, recursive = TRUE))
  case <- file.path(root, "negative-function-contract")
  entry <- file.path(case, "spec.yaml")
  writeLines(sub("name: project_value", "name: project_value\n        args: {x: SOURCE.VALUE}",
    readLines(entry), fixed = TRUE), entry)
  runs <- yamaa_project(file.path(case, "project.yaml"))$run()
  stopifnot("the project run has no issues" = nrow(check_issues(runs$issues)) == 0L)
  unlink(file.path(case, "input"), recursive = TRUE); unlink(entry)
  held <- restored(case, "test.csv", runs$save)
  expect_identical(held[["test.csv"]], charToRaw("ID,RESULT\n01,1.5\n"), "a project run saves its held output")
  fixture <- file.path(define_fixtures, "sdtm-dm-metadata")
  directory <- file.path(root, "submission"); dir.create(directory)
  stopifnot("the submission fixture is copied" =
    all(file.copy(file.path(fixture, c("project.yaml", "spec.yaml")), directory)) &&
      file.copy(file.path(fixture, "expected", "dm.csv"), directory))
  created <- read_host_fixture(host_fixture_root, "public-define")[["created"]]
  submission <- yamaa_project(file.path(directory, "project.yaml"))$submission("sdtm", created = created)
  stopifnot("the submission has no issues" = nrow(check_issues(submission$issues)) == 0L)
  unlink(file.path(directory, c("project.yaml", "spec.yaml", "dm.csv")))
  held <- restored(directory, c("define.xml", "dm.json"), submission$save)
  for (name in names(held)) {
    expect_identical(held[[name]], rawfile(file.path(fixture, "expected", name)),
      paste("the submission saves the authored", name))
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
  # One function file holds the contract and its R callable; the project
  # names it and the R lock (D8). A section lists only specifications of its
  # model (SUB-3).
  writeBin(charToRaw("yamaa: \"1.0\"\ntype: function\nname: text\ndescription: Return text unchanged.\nparams: [{name: x, type: str}]\nreturns: str\nimplementations: {r: base::identity}\ntests:\n  - {id: normal, covers: [normal], args: {x: short}, result: short}\n  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"),
    file.path(directory, "text.yaml"))
  writeBin(charToRaw("yamaa: \"1.0\"\ntype: project\nfunctions: [text.yaml]\nenvironments: {r: {lock: renv.lock}}\nadam: {specs: [domain.yaml]}\n"),
    file.path(directory, "project.yaml"))
  writeBin(charToRaw("yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: TEXT\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, V: str}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: V, type: str, derivation: {function: {name: text, args: {x: SRC.V}}}}\noutput: {path: held.csv, columns: [ID, V]}\n"),
    file.path(directory, "domain.yaml"))
  value <- paste0(strrep("\u00e9", 524289L), "complete")
  expected <- charToRaw(paste0("ID,V\n1,", value, "\n"))
  writeBin(expected, file.path(directory, "input.csv"))
  built <- yamaa_project(file.path(directory, "project.yaml"))$spec("TEXT")$run()
  stopifnot("the large text run has no issues" = nrow(check_issues(built$issues)) == 0L,
    "the callback returns the whole text" = identical(built$output$V, value))
  unlink(file.path(directory, c("domain.yaml", "project.yaml", "text.yaml", "renv.lock", "input.csv")))
  stopifnot("the large text run saves" = returns_paths(built$save()),
    "the saved CSV holds the whole text" =
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
  expect_identical(conditionMessage(error), error_text("unread_project", absent),
    "selecting from an absent project gives D39's text")
  expect_identical(as.list(unread[c("condition", "spec_paths")]),
    list(condition = "resource_path_missing", spec_paths = list(character())),
    "an absent project's one issue")
  expect_identical(jsonlite::fromJSON(unread$context, simplifyVector = FALSE), list(path = absent),
    "an absent project's issue names the path given")
  expect_identical(check_issues(project$check()$issues), unread, "an absent project's check")
  error <- refusal(project$names)
  expect_identical(conditionMessage(error), error_text("unread_project_names", absent),
    "an absent project's names give D39's text")
  expect_identical(error$issues, unread, "an absent project's names carry its issue")
  runs <- project$run()
  expect_identical(runs$names, character(), "an absent project's run builds nothing")
  expect_identical(check_issues(runs$issues), unread, "an absent project's run carries its issue")
  expect_identical(names(runs$report), fixture_strings(host_truth[["report_columns"]]),
    "an absent project's report columns")
  expect_identical(nrow(runs$report), 0L, "an absent project's report is empty")
  expect_identical(members(runs), expected_members("project_run"), "a project run's members")
  locked(runs, c(fixture_strings(host_truth[["read_only_views"]][["project_run"]]), "save"))
  error <- refusal(runs$save())
  expect_identical(conditionMessage(error), error_text("refused_project_run_save", absent),
    "a refused project run's save text")
  expect_identical(error$issues, unread, "a refused project run's save carries its issue")
  error <- refusal(runs[["ADSL"]])
  expect_identical(conditionMessage(error), error_text("no_member_built"), "a project run that built nothing")
  stopifnot("a project run that built nothing names no member" = unknown_member(error, "[]"))
  submission <- project$submission("sdtm", created = "2026-09-11T00:00:00")
  expect_identical(members(submission), expected_members("submission"), "a submission's members")
  stopifnot("a submission names its members" = named(submission, "submission"))
  expect_identical(check_issues(submission$issues), unread, "an absent project's submission carries its issue")
  # The creation time given is kept, though nothing composed (D129).
  expect_identical(submission$created, "2026-09-11T00:00:00", "the creation time given is kept")
  locked(submission, c(fixture_strings(host_truth[["read_only_views"]][["submission"]]), "save"))
  for (attempt in 1:2) {
    error <- refusal(submission$save())
    label <- sprintf("save attempt %d of a refused submission", attempt)
    expect_identical(conditionMessage(error), error_text("refused_submission_save", absent),
      paste(label, "gives D39's text"))
    expect_identical(error$issues, submission$issues, paste(label, "carries the submission's issues"))
  }
  # The project file is read first: one that cannot be read reports its own
  # findings, and no section is checked.
  expect_identical(check_issues(project$run("bogus")$issues), unread,
    "an absent project's run of any section carries its issue")
  expect_identical(check_issues(project$submission("bogus")$issues), unread,
    "an absent project's submission of any section carries its issue")
  # A name that is no section of the project raises (PROJECT-14).
  listing <- file.path(root, "project.yaml")
  writeLines(c('yamaa: "1.0"', "type: project", "adam:", "  specs: []"), listing)
  project <- yamaa_project(listing)
  for (case in list(list(call = function() project$run("bogus"), message = "unknown_section_run"),
                    list(call = function() project$submission("bogus"), message = "unknown_section_submission"))) {
    error <- refusal(case$call())
    expect_identical(conditionMessage(error), error_text(case$message), paste("the text of", case$message))
    stopifnot("an unknown section's one issue" = unknown_section(error, "bogus"))
  }
  expect_identical(list.files(root), "project.yaml", "project refusals write nothing")
  cat("project refusals are issues, refused saves fail and unknown sections raise\n")
}
qualify_project_refusals()

qualify_issue_geometry <- function() {
  root <- tempfile("yamaa-issue-geometry-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  path <- function(name) file.path(root, name)
  # A literal beyond every 64-bit range is refused before study access; the
  # context text keeps the signed 64-bit bounds and the exact literal, as a
  # number and as its text.
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]", "input: {SRC: input.csv}",
    "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: N, type: int, derivation: 18446744073709551616}",
    "output: {path: output.csv, columns: [ID, N]}"), path("spec.yaml"))
  frame <- check_issues(yamaa_spec(path("spec.yaml"))$check()$issues)
  expect_identical(as.list(frame[c("phase", "condition", "rule", "spec_paths", "context", "source_span",
    "operand_route")]),
    list(phase = "validation", condition = "integer_overflow", rule = "VALUE-5",
      spec_paths = list("columns.N.derivation"),
      context = '{"actual":18446744073709551616,"maximum":9223372036854775807,"minimum":-9223372036854775808,"value":"18446744073709551616"}',
      source_span = list(NULL), operand_route = list(NULL)),
    "a literal beyond 64 bits keeps its exact digits and has no geometry")
  # A division by zero fails at one node of the expression: the span
  # covers that node's bytes and the route leads to it from the root, so a
  # failure at the root has a present empty route, never NULL.
  writeBin(charToRaw("ID,VALUE,ZERO\na,2.5,0\n"), path("input.csv"))
  for (case in list(list(expression = "1 + VALUE / ZERO", route = "right"),
                    list(expression = "VALUE / ZERO", route = character(0)))) {
    writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
      "input: {SRC: {path: input.csv, types: {ID: str, VALUE: float, ZERO: float}}}",
      "columns:", "  - {name: ID, type: str, derivation: SRC.ID}",
      "  - {name: VALUE, type: float, derivation: SRC.VALUE}",
      "  - {name: ZERO, type: float, derivation: SRC.ZERO}",
      paste0("  - {name: RATIO, type: float, derivation: '", case$expression, "'}"),
      "output: {path: output.csv, columns: [ID, VALUE, ZERO, RATIO]}"), path("spec.yaml"))
    result <- yamaa_spec(path("spec.yaml"))$run()
    frame <- check_issues(result$issues)
    span <- frame$source_span[[1L]]
    label <- paste("a division by zero in", case$expression)
    expect_identical(result$output, NULL, paste(label, "leaves no output"))
    expect_identical(frame$condition, "division_by_zero", paste(label, "is its condition"))
    expect_identical(substr(case$expression, span[[1L]] + 1, span[[2L]]), "VALUE / ZERO",
      paste(label, "spans the failing node"))
    expect_identical(frame$operand_route[[1L]], case$route, paste(label, "routes to the failing node"))
    stopifnot("the context names the row's key" = grepl('"keys":[{"ID":"a"}]', frame$context, fixed = TRUE))
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
    # `{path}` stands for the specification path given, and `{entry}` for its
    # absolute path as the engine resolves it, with forward slashes, each as
    # JSON text in the context.
    given <- c(message = spec, context = gsub("\\", "\\\\", spec, fixed = TRUE))
    entry <- normalizePath(spec, winslash = "/", mustWork = FALSE)
    expected <- lapply(expected, function(issue) {
      for (field in names(given)) if (!is.null(issue[[field]]))
        issue[[field]] <- gsub("{path}", given[[field]], issue[[field]], fixed = TRUE)
      if (!is.null(issue$context))
        issue$context <- gsub("{entry}", entry, issue$context, fixed = TRUE)
      issue
    })
    # A truth row names its file relative to the case's directory; a
    # location names it by its absolute path (P-ISSUE-1).
    expect_identical(issue_columns_of(frame),
      issue_truth(expected, normalizePath(directory, winslash = "/", mustWork = TRUE)),
      paste("the issue rows of the shared parity case", case$name))
  }
  cat("issue rows equal the shared parity truth field for field\n")
}
qualify_issue_parity()

# A value-level entry that selects no row is a warning in the run's report,
# never an issue (SUB-13); Python compares the same rows. COUNT and TOTAL are
# doubles in R, and a missing cell is NA.
qualify_value_level_report <- function() {
  root <- tempfile("yamaa-value-level-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  case <- host_truth[["value_level_report"]]
  spec <- file.path(root, "spec.yaml")
  writeBin(charToRaw(enc2utf8(case$specification_yaml)), spec)
  writeBin(charToRaw(enc2utf8(case$input_csv)), file.path(root, "input.csv"))
  run <- yamaa_spec(spec)$run()
  stopifnot(nrow(check_issues(run$issues)) == 0L)
  report <- run$report
  checks <- report[!is.na(report$NAME) & report$NAME == "value_level", , drop = FALSE]
  columns <- fixture_strings(host_truth[["report_columns"]])
  cells <- function(name) lapply(case$checks, `[[`, name)
  expected <- lapply(columns, function(name) {
    if (name %in% c("COUNT", "TOTAL")) {
      vapply(cells(name), function(value) if (is.null(value)) NA_real_ else as.double(value), 0)
    } else {
      vapply(cells(name), function(value) if (is.null(value)) NA_character_ else value, "")
    }
  })
  actual <- lapply(columns, function(name) checks[[name]])
  names(expected) <- names(actual) <- columns
  if (!identical(actual, expected)) {
    str(actual); str(expected)
    stop("value-level report rows differ from the shared truth")
  }
  stopifnot(identical(checks$CONDITION[checks$OUTCOME == "violated"], case$condition))
  cat("a value-level entry that selects no row warns in the run's report\n")
}
qualify_value_level_report()

qualify_resolve <- function() {
  root <- tempfile("yamaa-resolve-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE), add = TRUE)
  root <- normalizePath(root, winslash = "/")
  entry <- file.path(root, "spec.yaml")
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing/raw.csv, types: {ID: int}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: absent/output.csv, columns: [ID]}"), entry)
  spec <- yamaa_spec(entry)
  facts <- spec$resolve()
  resource <- Filter(function(value) identical(value$role, "input_data"), facts$entries[[1L]]$resources)[[1L]]
  stopifnot("the facts are a list" = is.list(facts))
  expect_identical(facts[c("format", "artifact_version", "entry", "replay")],
    list(format = "yamaa.resolved-specification", artifact_version = 1, entry = entry,
      replay = "inspection_only"),
    "the resolved facts' identity")
  stopifnot("the facts name no project, as NULL" = "project" %in% names(facts) && is.null(facts[["project"]]))
  expect_identical(resource$location, list(kind = "local", path = file.path(root, "missing/raw.csv")),
    "an input's location")
  stopifnot("resolve creates no output directory" = !dir.exists(file.path(root, "absent")))
  unlink(entry)
  error <- refusal(spec$resolve())
  expect_identical(conditionMessage(error), error_text("resolve", entry), "resolving a removed specification")
  expect_identical(error$issues$condition, "resource_path_missing", "a removed specification's issue")

  # The project supplies the function and names itself; the same file alone
  # lacks that binding (D32, D146). No callable is activated. A section lists
  # only specifications of its model (SUB-3).
  writeLines(c('yamaa: "1.0"', "type: spec", "model: ADaM", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}, {name: V, type: str, derivation: {function: {name: text, args: {x: RAW.V}}}}]",
    "output: {path: output.csv, columns: [ID, V]}"), entry)
  writeLines(c('yamaa: "1.0"', "type: function", "name: text", "description: Identity.",
    "params: [{name: x, type: str}]", "returns: str",
    "implementations: {python: uninstalled_resolved_fixture.identity, r: uninstalledresolvedfixture::identity}",
    "tests:",
    "  - {id: normal, covers: [normal], args: {x: a}, result: a}",
    "  - {id: empty, covers: [boundary], args: {x: ''}, result: ''}",
    "  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}"),
    file.path(root, "text.yaml"))
  writeLines(c('yamaa: "1.0"', "type: project", "functions: [text.yaml]",
    "adam: {specs: [spec.yaml]}"), file.path(root, "project.yaml"))
  refuse <- function(...) stop("unexpected activation")
  facts <- with_capabilities(refuse, refuse,
    yamaa_project(file.path(root, "project.yaml"))$spec("TEST")$resolve())
  expect_identical(facts[c("project", "entry")], list(project = file.path(root, "project.yaml"), entry = entry),
    "a project's specification names the project")
  error <- refusal(yamaa_spec(entry)$resolve())
  expect_identical(as.list(error$issues[c("condition", "rule", "spec_paths", "context")]),
    list(condition = "project_binding_missing", rule = "CT-13",
      spec_paths = list("columns.V.derivation.function.name"),
      context = '{"binding":"function","function":"text"}'),
    "the specification alone lacks its project's function")

  # A mapping key holding NUL is both text and a name in the facts, and R
  # holds neither: resolve refuses it instead of failing inside R.
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]",
    "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}",
    "columns: [{name: ID, type: int, derivation: RAW.ID}, {name: W, type: str, derivation: {mapping: {source: RAW.V, dict: {\"L\\0\": LOW}}}}]",
    "output: {path: output.csv, columns: [ID, W]}"), entry)
  error <- refusal(yamaa_spec(entry)$resolve())
  stopifnot("resolve refuses a key holding NUL" = grepl("containing NUL cannot be held by an R character vector",
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
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: PRODUCER", "keys: [ID]",
    "input: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}",
    "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]",
    "output: {path: output/produced.csv, relative_to: entry, columns: [ID]}"), file.path(root, "shared/producer.yaml"))
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: CONSUMER", "keys: [ID]",
    "input: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}",
    "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]",
    "output: {path: output/final.csv, relative_to: entry, columns: [ID]}"), file.path(root, "shared/consumer.yaml"))
  for (wrapper in c("A", "B")) {
    study <- file.path(root, wrapper)
    dir.create(file.path(study, "input"), recursive = TRUE)
    identifier <- if (identical(wrapper, "A")) 17 else 29
    writeLines(c("ID", identifier), file.path(study, "input/raw.csv"))
    writeLines(c('yamaa: "1.0"', "type: spec", "parent: ../shared/consumer.yaml"), file.path(study, "spec.yaml"))
    writeLines(c('yamaa: "1.0"', "type: spec", "parent: ../shared/producer.yaml"), file.path(study, "producer.yaml"))
    spec <- yamaa_spec(file.path(study, "spec.yaml"))
    entries <- spec$resolve()$entries
    # A linked producer is type metadata, not a resolved entry.
    expect_identical(length(entries), 1L, paste("study", wrapper, "resolves one entry"))
    stopifnot("a linked producer is no resolved entry" = is.null(entries[[1L]]$producers))
    schema <- Filter(function(value) identical(value$role, "producer_schema"), entries[[1L]]$resources)[[1L]]
    expect_identical(schema$location$path, file.path(study, "producer.yaml"),
      paste("study", wrapper, "links its own producer"))
    source <- Filter(function(value) identical(value$role, "input_data"), entries[[1L]]$resources)[[1L]]
    expect_identical(source$location$path, file.path(study, "output/produced.csv"),
      paste("study", wrapper, "reads its own producer's output"))
    # Callers build and save each study's producer first.
    dir.create(file.path(study, "output"))
    stopifnot("the producer saves" = returns_paths(yamaa_spec(file.path(study, "producer.yaml"))$run()$save()))
    result <- spec$run()
    expect_identical(nrow(result$issues), 0L, paste("study", wrapper, "runs without issues"))
    expect_identical(result$output$ID, identifier, paste("study", wrapper, "reads its own identifier"))
    stopifnot("the consumer writes nothing before save" = !file.exists(file.path(study, "output/final.csv")))
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
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]", "input: {RAW: missing.csv}",
    "columns: [{name: ID, type: str, derivation: {mapping: {source: RAW.ID, dict: {A: Alpha}}}}]",
    "output: {path: result.csv, columns: [ID]}"), base)
  writeLines(c('yamaa: "1.0"', "type: spec", "parent: base.yaml"), child)
  origins <- function(path) yamaa_spec(path)$resolve()$entries[[1L]]$origins
  direct <- origins(base)
  inherited <- origins(child)
  pointer <- "/columns/0/derivation/value/mapping/case_sensitive"
  expect_identical(direct[[pointer]]$source_kind, "schema_default", "a default's origin")
  expect_identical(inherited[[pointer]], direct[[pointer]], "an inherited default keeps its origin")
  stopifnot("a shorthand input is generated" = isTRUE(inherited[["/input/RAW"]]$generated))
  writeLines(c('yamaa: "1.0"', "type: spec", "domain: TEST", "keys: [ID]", "input: {RAW: missing.csv}",
    "windows: {W: {order_by: [RAW.ID]}}", "columns: [{name: ID, type: int, derivation: RAW.ID}]",
    "output: {path: result.csv, columns: [ID]}"), base)
  writeLines(c('yamaa: "1.0"', "type: spec", "parent: base.yaml",
    "columns: [{name: N, type: int, derivation: {row_number: {window: W}}}]",
    "output: {path: result.csv, columns: [ID, N]}"), child)
  inherited <- origins(child)
  prefix <- "/columns/1/derivation/value/row_number/window"
  expect_identical(inherited[[prefix]]$declaring_source, child, "the window reference's origin")
  expect_identical(inherited[[paste0(prefix, "/order_by/0/variable")]]$declaring_source, base,
    "the named window's definition keeps its origin")
  expect_identical(inherited[[paste0(prefix, "/order_by/0/direction")]]$source_kind, "schema_default",
    "the window's default direction")
  # A 1.0 derivation is one expression (D18): its written value stays
  # authored, and only the derivation that wraps it is generated.
  stopifnot("the derivation that wraps an expression is generated" =
    isTRUE(inherited[["/columns/0/derivation"]]$generated),
    "the written expression stays authored" =
    isFALSE(inherited[["/columns/0/derivation/value"]]$generated))
  cat("resolved original origins retain inherited defaults, shorthand and named-window definitions\n")
}
qualify_resolved_origins()
