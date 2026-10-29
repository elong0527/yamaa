# Installed native handle lifetime, counted by the binding's census,
# yamaa:::.native_live_handles(), in the order of the shared truth's
# interface.census_kinds, which Python reads too. Dropping a result releases
# its handle, a run restored by readRDS refuses as expired, and a refused
# conversion or a cancelled run or save leaves no handle behind. Each count
# is derived from what a step makes: a lazy object's first run makes its
# plans handle, and a result holds one handle of its kind; R copies tables
# into data frames, so it holds no table handle.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped handle_lifetime.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
projects <- normalizePath("project-base", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "reader.R"), local = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- read_host_fixture(host_fixture_root, "interface")
lifetime <- truth$lifetime

# The live native handles once every unreachable object is collected.
live <- function() {
  invisible(gc())
  yamaa:::.reply(yamaa:::.native_live_handles())
}
plus <- function(counts, kind) {
  counts[[kind]] <- counts[[kind]] + 1L
  counts
}
# A new directory holding the lifetime study.
study <- function() {
  root <- tempfile("yamaa-lifetime-")
  dir.create(root)
  writeBin(charToRaw(lifetime$specification_yaml), file.path(root, "spec.yaml"))
  writeBin(charToRaw(lifetime$input_csv), file.path(root, "input.csv"))
  writeBin(charToRaw(lifetime$project_yaml), file.path(root, "project.yaml"))
  root
}

expect_identical(names(live()), fixture_strings(truth$census_kinds), "the census counts each kind in order")
cat("the census counts each kind in order\n")

check_dropping <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  spec <- yamaa_spec(file.path(root, "spec.yaml"))
  project <- yamaa_project(file.path(root, "project.yaml"))
  # Each lazy object's first run makes its plans handle.
  invisible(spec$run())
  invisible(project$run())
  cases <- list(
    list(label = "a lazy object after its first run", kind = "plans", make = function() {
      object <- yamaa_spec(file.path(root, "spec.yaml"))
      invisible(object$run())
      object
    }),
    list(label = "a check", kind = "check", make = function() spec$check()),
    list(label = "a run", kind = "run", make = function() spec$run()),
    list(label = "a project run", kind = "project_run", make = function() project$run()),
    list(label = "a submission", kind = "submission", make = function() project$submission()),
    list(label = "a lint result", kind = "lint", make = function() yamaa_lint(file.path(root, "spec.yaml"))),
    list(label = "a selftest", kind = "selftest", make = function() yamaa_selftest()))
  for (case in cases) {
    before <- live()
    made <- case$make()
    held <- live()
    rm(made)
    expect_identical(list(held = held, after = live()), list(held = plus(before, case$kind), after = before),
      paste("dropping", case$label, "releases its native handle"))
  }
  cat("dropping each result releases its native handle\n")

  before <- live()
  run <- spec$run()
  saved <- file.path(root, "run.rds")
  saveRDS(run, saved)
  restored <- readRDS(saved)
  seen <- tryCatch(restored$status, yamaa_error = identity)
  held <- live()
  rm(run, restored)
  expect_identical(
    list(message = conditionMessage(seen), held = held, after = live()),
    list(message = "expired native handle", held = plus(before, "run"), after = before),
    "a run restored by readRDS refuses as expired")
  cat("a run restored by readRDS refuses as expired\n")
}
check_dropping()

check_conversions <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  # Each reason is the binding's refusal of a value no plain R vector holds.
  cases <- list(
    list(label = "an integer beyond 2^53", input = charToRaw("ID,NAME\n9007199254740993,alpha\n"),
      reason = "integer 9007199254740993 is beyond 2^53 and cannot be held exactly by an R double"),
    list(label = "text holding NUL", input = c(charToRaw("ID,NAME\n1,a"), as.raw(0L), charToRaw("b\n")),
      reason = "text containing NUL cannot be held by an R character vector"))
  for (case in cases) {
    writeBin(case$input, file.path(root, "input.csv"))
    before <- live()
    run <- yamaa_spec(file.path(root, "spec.yaml"))$run()
    seen <- tryCatch(run$output, yamaa_error = identity)
    held <- live()
    rm(run)
    expect_identical(list(class = class(seen), message = conditionMessage(seen), held = held, after = live()),
      list(class = c("yamaa_error", "error", "condition"), message = case$reason, held = plus(before, "run"),
        after = before),
      paste("a refused conversion of", case$label, "leaves no native handle"))
  }
  cat("a refused conversion leaves no native handle\n")
}
check_conversions()

check_cancellation <- function() {
  root <- study()
  on.exit(unlink(root, recursive = TRUE))
  case <- function_study(projects, root)
  spec <- yamaa_project(file.path(case, "project.yaml"))$spec("TEST")
  invisible(spec$run())
  capabilities <- get(".yamaa_locked_host_capabilities", asNamespace("yamaa"))()
  interrupted <- structure(list(message = "interrupted", call = NULL), class = c("interrupt", "condition"))
  # The project function signals an interrupt each time it is called.
  resolve <- function(call, parameters) {
    resolved <- capabilities$resolve(call, parameters)
    if (resolved[[1L]] != 0L) return(resolved)
    list(0L, function(arguments) list(3L, interrupted))
  }
  before <- live()
  seen <- with_capabilities(capabilities$verify, resolve, tryCatch(spec$run(), interrupt = identity))
  expect_identical(list(interrupt = identical(seen, interrupted), live = live()),
    list(interrupt = TRUE, live = before), "a cancelled run leaves no native handle")
  # Windows cannot send SIGINT to the current process with tools::pskill. This
  # case is best effort: the engine polls the pending interrupt before the
  # save's first commit, but R may take it before the call reaches the engine,
  # which leaves the same. Python's cancelled save is the deterministic one.
  if (.Platform$OS.type == "unix") {
    run <- spec$run()
    seen <- tryCatch({
      tools::pskill(Sys.getpid(), signal = 2L)
      run$save()
    }, interrupt = identity)
    held <- live()
    published <- file.exists(file.path(case, "test.csv"))
    status <- run$status
    run$save()
    saved <- readBin(file.path(case, "test.csv"), "raw", 100L)
    rm(run)
    expect_identical(
      list(interrupt = inherits(seen, "interrupt"), held = held, published = published, status = status,
        saved = saved, after = live()),
      list(interrupt = TRUE, held = plus(before, "run"), published = FALSE, status = "succeeded",
        saved = charToRaw(lifetime$function_study_csv), after = before),
      "a cancelled save leaves no native handle, publishes nothing and keeps a run that saves later")
  } else {
    cat("SKIP: in-process SIGINT requires tools::pskill on Unix\n")
  }
  cat("a cancelled run and save leave no native handle\n")
}
check_cancellation()
