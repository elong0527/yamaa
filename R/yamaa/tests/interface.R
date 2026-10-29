# Installed interface handshake and native gate (rust/interface/README.md).
# The generated .onLoad negotiates before any other native routine, and every
# other routine refuses until a negotiation is accepted. The generated
# request, each crafted request's outcome, the message template of each
# reason and the gate's refusal are the shared truth's interface family,
# which Python reads too; each outcome was written by hand from the
# handshake's check order.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped interface.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "interface")
fields <- c("accepted", "requested_version", "supported_version", "reason", "missing_features")

handshake <- function(request) yamaa:::.reply(yamaa:::.native_interface_handshake(request))
# The native module's boundary version, which a refusal names; the generated
# request must ask for exactly it.
version <- handshake(list())$supported_version

# The truth with its placeholders filled: {version} is the boundary version,
# {next_version} the one after it and {core_version} the installed engine's
# release.
resolved <- function(value) {
  if (is.list(value)) return(lapply(value, resolved))
  if (identical(value, "{version}")) return(version)
  if (identical(value, "{next_version}")) return(version + 1L)
  if (identical(value, "{core_version}")) return(yamaa:::.core_version())
  value
}
# An authored outcome as the handshake returns it, its lists of text as
# character vectors.
outcome_of <- function(outcome) {
  for (field in c("enabled", "supported", "language_versions", "missing_features")) {
    if (field %in% names(outcome)) outcome[[field]] <- as.character(unlist(outcome[[field]]))
  }
  outcome
}
# The truth's message for a refusal: the template of its reason, with each
# field it names filled from the envelope.
message_of <- function(outcome) {
  text <- truth$messages[[outcome$reason]]
  for (field in c("requested_version", "supported_version")) {
    if (!is.null(outcome[[field]])) {
      text <- gsub(sprintf("{%s}", field), format(outcome[[field]], scientific = FALSE), text, fixed = TRUE)
    }
  }
  gsub("{missing_features}", paste(outcome$missing_features, collapse = ", "), text, fixed = TRUE)
}
refusal <- function(requested, supported, reason) {
  list(accepted = FALSE, requested_version = requested, supported_version = supported,
    reason = reason, missing_features = character())
}
crafted <- function(name) {
  for (case in truth$cases) if (identical(case$case, name)) return(resolved(case))
  stop("no interface case ", name)
}

check_interface <- function() {
  expect_identical(yamaa:::.interface_request,
    list(version = version, required = fixture_strings(truth$request$required),
      optional = fixture_strings(truth$request$optional)),
    "the generated request asks for the native module's version")
  accepted <- handshake(yamaa:::.interface_request)
  formats <- sub("^format_", "", grep("^format_", accepted$enabled, value = TRUE))
  expect_identical(list(accepted = accepted, formats = sort(formats)),
    list(accepted = outcome_of(resolved(truth$accepted)), formats = sort(yamaa_info()$features)),
    "the load negotiates the generated request")
  cat("the load negotiates the generated request\n")

  # Requests that only R builds: an NA is no version, a whole double is an
  # integer as R writes one, a character vector is a list of identifiers, a
  # list with names may repeat one, a name R cannot give as text names no
  # field but leaves the version to be read first, and nothing deeper than a
  # request's own shape is read.
  required <- fixture_strings(truth$request$required)
  unreadable <- list(version = version + 1L, name = list())
  names(unreadable)[[2L]] <- rawToChar(as.raw(c(0x61, 0xff)))
  deep <- list()
  for (index in seq_len(100000L)) deep <- list(deep)
  plain <- outcome_of(crafted("every required identifier and no optional one")$outcome)
  one <- plain
  one$enabled <- "structured_records"
  r_cases <- list(
    list(case = "an NA version is malformed",
      request = list(version = NA, required = required, optional = character()),
      outcome = refusal(NULL, version, "malformed_request")),
    list(case = "an integer NA version is malformed",
      request = list(version = NA_integer_, required = required, optional = character()),
      outcome = refusal(NULL, version, "malformed_request")),
    list(case = "a whole double version is an integer",
      request = list(version = as.double(version), required = required, optional = character()),
      outcome = plain),
    list(case = "a character vector of one identifier is a list of one",
      request = list(version = version, required = "structured_records", optional = character()),
      outcome = one),
    list(case = "a version given twice is malformed",
      request = list(version = version, version = version, required = required, optional = character()),
      outcome = refusal(NULL, version, "malformed_request")),
    list(case = "another version with a name R cannot give as text is a mismatch",
      request = unreadable, outcome = refusal(version + 1L, version, "version_mismatch")),
    list(case = "a list nested a hundred thousand deep is malformed",
      request = list(version = version, required = deep, optional = character()),
      outcome = refusal(version, version, "malformed_request")))
  shared <- lapply(truth$cases, function(case) {
    case <- resolved(case)
    list(case = case$case, request = case$request, outcome = outcome_of(case$outcome))
  })
  for (case in c(shared, r_cases)) {
    outcome <- handshake(case$request)
    expect_identical(outcome, case$outcome, case$case)
    if (!case$outcome$accepted) {
      error <- yamaa:::.interface_error(outcome)
      expect_identical(
        list(class = class(error), message = conditionMessage(error), fields = unclass(error)[fields]),
        list(class = c("yamaa_interface_error", "error", "condition"), message = message_of(case$outcome),
          fields = case$outcome),
        paste(case$case, "as a condition"))
    }
  }
  cat("each crafted request gets its authored outcome\n")

  missing <- refusal(version, NULL, "missing_interface")
  seen <- tryCatch(yamaa:::.interface_negotiate(function(name) NULL), yamaa_interface_error = identity)
  expect_identical(list(message = conditionMessage(seen), fields = unclass(seen)[fields]),
    list(message = message_of(missing), fields = missing),
    "a library without the handshake routine refuses the load as missing_interface")
  cat("a library without the handshake routine refuses the load as missing_interface\n")

  # A library of the next boundary version, whose lookup records each routine
  # the load asks for.
  asked <- character()
  next_version <- function(request) {
    list(value = refusal(request$version, request$version + 1L, "version_mismatch"), error = NULL)
  }
  seen <- tryCatch(yamaa:::.interface_negotiate(function(name) {
    asked <<- c(asked, name)
    next_version
  }), yamaa_interface_error = identity)
  mismatch <- refusal(version, version + 1L, "version_mismatch")
  expect_identical(list(asked = asked, message = conditionMessage(seen), fields = unclass(seen)[fields]),
    list(asked = "interface_handshake", message = message_of(mismatch), fields = mismatch),
    "a refused handshake stops the load before any other routine")
  cat("a refused handshake stops the load before any other routine\n")

  refused <- handshake(crafted("a request for the next version is a mismatch")$request)
  expect_identical(list(reason = refused$reason, info = yamaa_info()$version),
    list(reason = "version_mismatch", info = yamaa:::.core_version()),
    "a direct handshake call leaves the negotiation unchanged")
  cat("a direct handshake call leaves the negotiation unchanged\n")
}
check_interface()

# A fresh R process loads the library with dyn.load, without the package, and
# calls each registered routine but the handshake and the core version before
# negotiation, after a refused request and after an accepted one. A routine
# that takes a handle gets NULL, which no negotiation makes a handle. extendr's
# own routines, which describe each module and write its R wrappers, are
# framework machinery that reaches no engine code (rust/interface/README.md).
check_gate <- function() {
  work <- tempfile("yamaa-gate-")
  dir.create(work)
  on.exit(unlink(work, recursive = TRUE))
  path <- charToRaw("missing.yaml")
  r_version <- charToRaw(as.character(getRversion()))
  handles <- c("check_status", "check_issues", "check_timings", "check_summary", "plans", "run_status",
    "run_output", "run_issues", "run_report", "run_timings", "run_save", "run_summary",
    "project_run_status", "project_run_names", "project_run_issues", "project_run_report",
    "project_run_timings", "project_run_save", "project_run_summary", "submission_status",
    "submission_created", "submission_issues", "submission_timings", "submission_save",
    "submission_summary", "selftest_status", "selftest_cases", "selftest_info", "selftest_summary")
  members <- c("project_run_member_status", "project_run_output", "project_run_member_issues",
    "project_run_member_report", "project_run_member_timings", "project_run_member_summary")
  arguments <- c(
    setNames(rep(list(list(NULL)), length(handles)), handles),
    setNames(rep(list(list(NULL, charToRaw("TEST"))), length(members)), members),
    list(run_observations = list(NULL, FALSE),
      spec_check = list(path, NULL, NULL, r_version, NULL),
      spec_summary = list(path),
      spec_run = list(path, NULL, r_version, list(), NULL, FALSE, NULL),
      spec_resolve = list(path, NULL, r_version),
      read_odm = list(charToRaw("missing.xml"), NULL, 10000, 268435456, 64),
      project_spec = list(path, charToRaw("TEST"), r_version),
      project_summary = list(path),
      project_names = list(path, r_version),
      project_check = list(path, r_version, NULL),
      project_run = list(path, NULL, r_version, list(), NULL, NULL),
      project_submission = list(path, NULL, NULL, r_version, NULL),
      lint = list(list(path), FALSE),
      empty_issues = list(),
      schema = list(charToRaw("spec")),
      conditions = list(),
      info = list(r_version),
      selftest = list(r_version),
      live_handles = list()))
  input <- file.path(work, "input.rds")
  output <- file.path(work, "output.rds")
  saveRDS(list(arguments = arguments,
    refused = crafted("a request for the next version is a mismatch")$request,
    accepted = yamaa:::.interface_request), input)
  child <- file.path(work, "child.R")
  writeLines(c(
    "arguments <- commandArgs(trailingOnly = TRUE)",
    "routines <- getDLLRegisteredRoutines(dyn.load(arguments[[1L]]))$.Call",
    "input <- readRDS(arguments[[2L]])",
    "answer <- function(name, values) {",
    "  reply <- do.call(.Call, c(list(routines[[paste0(\"wrap__\", name)]]), values))",
    "  if (is.null(reply$error)) NULL else list(message = reply$error$message,",
    "    frame = is.data.frame(reply$error$issues), columns = names(reply$error$issues),",
    "    rows = nrow(reply$error$issues), paths = reply$error$paths, state = reply$error$state)",
    "}",
    "phase <- function() setNames(lapply(names(input$arguments), function(name) answer(name, input$arguments[[name]])), names(input$arguments))",
    "phases <- list(routines = sort(names(routines)), before = phase())",
    "invisible(answer(\"interface_handshake\", list(input$refused)))",
    "phases$refused <- phase()",
    "invisible(answer(\"interface_handshake\", list(input$accepted)))",
    "phases$accepted <- phase()",
    "saveRDS(phases, arguments[[3L]])"), child)
  libs <- system.file("libs", package = "yamaa", mustWork = TRUE)
  if (nzchar(.Platform$r_arch)) libs <- file.path(libs, .Platform$r_arch)
  library_path <- file.path(libs, paste0("yamaa", .Platform$dynlib.ext))
  quote <- function(path) shQuote(path, type = if (.Platform$OS.type == "windows") "cmd" else "sh")
  rscript <- file.path(R.home("bin"), if (.Platform$OS.type == "windows") "Rscript.exe" else "Rscript")
  # R CMD check names its test startup file in R_TESTS, relative to its own
  # directory; the child needs none.
  tests <- Sys.getenv("R_TESTS", unset = NA)
  Sys.setenv(R_TESTS = "")
  status <- system2(rscript, c("--vanilla", quote(child), quote(library_path), quote(input), quote(output)))
  if (is.na(tests)) Sys.unsetenv("R_TESTS") else Sys.setenv(R_TESTS = tests)
  stopifnot("the child process succeeds" = identical(status, 0L))
  phases <- readRDS(output)
  machinery <- "^wrap__(get_[a-z_]+_metadata|make_[a-z_]+_wrappers)$"
  names <- sort(names(arguments))
  refusal <- list(message = truth$gate_message, frame = TRUE,
    columns = fixture_strings(read_host_fixture(host_fixture_root, "public-contract")$issue_columns),
    rows = 0L, paths = character(), state = TRUE)
  gated <- setNames(rep(list(refusal), length(names)), names)
  expect_identical(
    list(routines = grep(machinery, phases$routines, value = TRUE, invert = TRUE),
      before = phases$before[names], refused = phases$refused[names],
      accepted = names(which(vapply(phases$accepted, identical, logical(1L), refusal)))),
    list(routines = sort(paste0("wrap__", c(names, "interface_handshake", "core_version"))),
      before = gated, refused = gated, accepted = character()),
    "every native routine refuses before negotiation")
  cat("every native routine refuses before negotiation\n")
}
check_gate()
