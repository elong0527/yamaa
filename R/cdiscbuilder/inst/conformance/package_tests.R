# Run the complete package suite with the repository fixtures available.
# From the repository root: Rscript R/cdiscbuilder/inst/conformance/package_tests.R
arguments <- commandArgs(trailingOnly = FALSE)
script <- sub("^--file=", "", arguments[grepl("^--file=", arguments)])
root <- normalizePath(file.path(dirname(script), "..", "..", "..", ".."))
package <- file.path(root, "R", "cdiscbuilder")

# sqldf's helper supports an R engine; no graphical Tcl/Tk session is needed.
options(gsubfn.engine = "R")
description <- read.dcf(file.path(package, "DESCRIPTION"))
declared <- paste(description[1, c("Imports", "Suggests")], collapse = ",")
required <- unique(c(
  trimws(gsub("\\s*\\([^)]*\\)", "", strsplit(declared, ",")[[1]])),
  "pkgload"
))
missing <- required[!vapply(required, requireNamespace, logical(1), quietly = TRUE)]
if (length(missing)) {
  stop("Install package test dependencies first: ", paste(missing, collapse = ", "))
}
cat("Repository:", root, "\n")
results <- testthat::test_local(
  package, reporter = "summary", stop_on_failure = FALSE
)
summary <- as.data.frame(results)
cat(
  "Expectations passed:", sum(summary$passed),
  "Failures:", sum(summary$failed),
  "Errors:", sum(summary$error),
  "Skipped tests:", sum(summary$skipped), "\n"
)
print(sessionInfo())
if (any(summary$failed > 0 | summary$error)) {
  stop("R package tests failed")
}
