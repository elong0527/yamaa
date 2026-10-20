# Installed Parquet containment: parquet 55.2.0's metadata decoder panics on
# an INT96 statistic longer than 12 bytes. The engine contains that panic as
# source_parquet_invalid (PARQUET-5), prints nothing, and later reads in the
# same process still work. On Windows this runs the GNU build, whose unwinding
# no Rust test reaches. Python reads the same truth.
library(yamaa)
# save() returns the absolute paths it wrote, invisibly, with forward slashes,
# in publication order (D38); each one exists once written.
returns_paths <- function(paths) is.character(paths) && length(paths) > 0L && !anyNA(paths) &&
  all(grepl("^(/|[[:alpha:]]:/)", paths)) && !any(grepl("\\", paths, fixed = TRUE)) &&
  all(file.exists(paths))
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped parquet_containment.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
truth <- read_host_fixture(host_fixture_root, "parquet-containment")

work <- tempfile("yamaa-parquet-containment-"); dir.create(work)
spec_path <- function(name) {
  directory <- file.path(work, name); dir.create(directory)
  writeBin(charToRaw(enc2utf8(truth[["specification_yaml"]])), file.path(directory, "spec.yaml"))
  writeBin(fixture_bytes(truth[[name]]), file.path(directory, "input.parquet"))
  normalizePath(file.path(directory, "spec.yaml"), winslash = "/", mustWork = TRUE)
}
contained <- spec_path("contained_panic_parquet")
readable <- spec_path("readable_parquet")
saved <- file.path(dirname(readable), "output.csv")
readable_csv <- charToRaw(enc2utf8(truth[["readable_csv"]]))
saved_csv <- function() readBin(saved, "raw", file.size(saved))

# Field for field, as public_contract.R compares the shared parity truth.
issues_equal <- function(frame, expected) {
  text <- function(field) vapply(expected, function(issue)
    if (is.null(issue[[field]])) NA_character_ else issue[[field]], "")
  actual <- list(frame$phase, frame$condition, frame$rule, frame$spec_paths,
    frame$context, frame$source_span, frame$operand_route)
  authored <- list(text("phase"), text("condition"), text("rule"),
    lapply(expected, function(issue) as.character(unlist(issue$spec_paths))),
    enc2utf8(text("context")),
    lapply(expected, function(issue)
      if (is.null(issue$source_span)) NULL else as.double(unlist(issue$source_span))),
    lapply(expected, function(issue)
      if (is.null(issue$operand_route)) NULL else as.character(unlist(issue$operand_route))))
  if (!identical(actual, authored)) { str(actual); str(authored) }
  identical(actual, authored)
}

for (attempt in 1:2) {
  result <- yamaa_spec(contained)$run()
  failure <- tryCatch({result$save(); NULL}, yamaa_error = identity)
  stopifnot(issues_equal(result$issues, truth[["issues"]]), is.null(result$output),
    inherits(failure, "yamaa_error"))
  result <- yamaa_spec(readable)$run()
  stopifnot(nrow(result$issues) == 0L, returns_paths(result$save()), identical(saved_csv(), readable_csv))
}
cat("a decoder panic is source_parquet_invalid, and later reads in the process still work\n")

# A fresh R process runs both specifications; its stderr is the one a panic
# hook writes to. It saves the run that has output and prints each run's
# conditions.
unlink(saved)
child <- file.path(work, "child.R")
writeLines(c(
  paste0(".libPaths(", paste(deparse(.libPaths()), collapse = ""), ")"),
  "suppressPackageStartupMessages(library(yamaa))",
  "for (path in commandArgs(trailingOnly = TRUE)) {",
  "  result <- yamaa_spec(path)$run()",
  "  if (!is.null(result$output)) result$save()",
  "  writeLines(paste(result$issues$condition, collapse = \" \"))",
  "}"), child)
quote <- function(path) shQuote(path, type = if (.Platform$OS.type == "windows") "cmd" else "sh")
out <- file.path(work, "child-stdout.txt"); err <- file.path(work, "child-stderr.txt")
rscript <- file.path(R.home("bin"), if (.Platform$OS.type == "windows") "Rscript.exe" else "Rscript")
# R CMD check names its test startup file in R_TESTS, relative to its own
# directory; the child needs none.
tests <- Sys.getenv("R_TESTS", unset = NA)
Sys.setenv(R_TESTS = "")
status <- system2(rscript, c("--vanilla", quote(child), quote(contained), quote(readable)),
  stdout = out, stderr = err)
if (is.na(tests)) Sys.unsetenv("R_TESTS") else Sys.setenv(R_TESTS = tests)
printed <- readLines(err)
if (length(printed)) cat(printed, sep = "\n")
conditions <- vapply(truth[["issues"]], function(issue) issue[["condition"]], "")
stopifnot(identical(status, 0L), file.size(err) == 0,
  identical(readLines(out), c(paste(conditions, collapse = " "), "")),
  identical(saved_csv(), readable_csv))
cat("a contained decoder panic prints nothing to stderr\n")
unlink(work, recursive = TRUE)
