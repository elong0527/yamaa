# CRAN's installed-package smoke checks. The embedded selftest covers the
# format adapters; the existing base-round study exercises the public R
# project/specification API, R function activation and saved bytes. The full
# supplemental suites run separately in CI against the same installed package.
# These checks use base R only, including when Suggests are unavailable.
library(yamaa)

selftest <- yamaa_selftest()
stopifnot("the bundled format cases pass" = identical(selftest$status, "succeeded"),
  "every bundled case runs" = nrow(selftest$cases) > 0L &&
    all(selftest$cases$status == "succeeded"),
  "the selftest identifies the installed engine" = identical(selftest$info, yamaa_info()))

smoke_project <- function() {
  root <- tempfile("yamaa-smoke-")
  dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  stopifnot("the authored study is copied" =
    file.copy("project-base/base-round", root, recursive = TRUE))
  study <- file.path(root, "base-round")
  project <- yamaa_project(file.path(study, "project.yaml"))
  specification <- project$spec("TEST")
  stopifnot("the project checks" = nrow(project$check()$issues) == 0L,
    "the specification checks" = nrow(specification$check()$issues) == 0L,
    "lint returns findings" = is.data.frame(yamaa_lint(study)$findings))

  events <- character()
  runs <- project$run(progress = function(event) events <<- c(events, event$kind))
  output <- file.path(study, "test.csv")
  expected <- file.path(study, "expected/test.csv")
  bytes <- function(path) readBin(path, "raw", n = file.info(path)$size)
  stopifnot("the project builds its dataset" = identical(runs$names, "TEST"),
    "the run has no findings" = nrow(runs$issues) == 0L,
    "the callback reports phase starts and ends" = all(c("start", "end") %in% events),
    "the result holds phase timings" = nrow(runs$timings) > 0L,
    "nothing is published before save" = !file.exists(output))
  invisible(runs$save())
  stopifnot("the R function's output matches the authored CSV" =
    identical(bytes(output), bytes(expected)))

  # A retained spec object also runs independently through the R binding.
  run <- specification$run()
  stopifnot("the specification returns plain R columns" =
    identical(as.data.frame(lapply(run$output, as.character)),
      utils::read.csv(expected, colClasses = "character", na.strings = "")))

  # The R callback error must become a diagnostic, not escape across Rust.
  unlink(output)
  refused <- specification$run(progress = function(event) stop("smoke callback failure"))
  failure <- tryCatch({refused$save(); NULL}, yamaa_error = identity)
  stopifnot("a callback error returns a diagnostic" =
    identical(refused$issues$condition, "engine_rejected"),
    "a failed run refuses publication" = inherits(failure, "yamaa_error"),
    "a failed run publishes nothing" = !file.exists(output))
}

smoke_project()
cat("CRAN smoke checks: embedded formats, public R APIs, callbacks and saved bytes passed\n")
