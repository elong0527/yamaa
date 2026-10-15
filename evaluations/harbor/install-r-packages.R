#!/usr/bin/env Rscript
# One runtime dependency setup for Harbor images and reference-solution CI.
arguments <- commandArgs(trailingOnly = TRUE)
if (!identical(arguments, character()) && !identical(arguments, "--check")) {
    stop("usage: Rscript install-r-packages.R [--check]")
}
packages <- c(
    "dplyr", "tidyr", "readr", "stringr", "lubridate", "purrr", "haven",
    "xml2", "nanoparquet"
)
if (!identical(arguments, "--check")) {
    options(repos = c(CRAN = "https://p3m.dev/cran/__linux__/noble/2026-09-27"))
    install.packages(packages, Ncpus = 4)
}
missing <- packages[!vapply(packages, requireNamespace, logical(1), quietly = TRUE)]
if (length(missing)) {
    stop(paste("Harbor R packages unavailable:", paste(missing, collapse = ", ")))
}
