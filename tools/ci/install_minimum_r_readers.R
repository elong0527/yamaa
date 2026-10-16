# Install the declared reader floors as prebuilt Posit binaries after their
# ordinary dependencies. The floors are not compiled from CRAN source: the
# declared arrow floor's vendored cpp11 glue does not build against the
# minimum R's headers. The snapshot below is the first Posit snapshot that
# serves R 4.6 binaries of the declared floor line (arrow 23.0.1.2, the
# 23.0.1 release line, and jsonlite 2.0.0); move it when the floors move.
# Run qualification in a fresh process so replaced namespaces cannot remain loaded.
arguments <- commandArgs(trailingOnly = TRUE)
description <- if (length(arguments)) arguments[[1L]] else "R/yamaa/DESCRIPTION"
suggests <- strsplit(read.dcf(description)[1L, "Suggests"], ",", fixed = TRUE)[[1L]]
declarations <- trimws(suggests)
stopifnot(all(grepl("^[a-zA-Z][a-zA-Z0-9.]* \\(>= [0-9.]+\\)$", declarations)))
packages <- sub(" .*", "", declarations)
versions <- sub(".*>= ([0-9.]+)\\)$", "\\1", declarations)
library <- .libPaths()[[1L]]
snapshot <- "https://packagemanager.posit.co/cran/__linux__/noble/2026-04-06"
options(HTTPUserAgent = sprintf(
  "R/%s R (%s)",
  getRversion(),
  paste(getRversion(), R.version["platform"], R.version["arch"], R.version["os"])
))
available <- available.packages(repos = c(CRAN = snapshot))
for (i in seq_along(packages)) {
  package <- packages[[i]]; floor <- versions[[i]]
  stopifnot(package %in% rownames(available))
  pinned <- available[package, "Version"]
  stopifnot(package_version(pinned) >= package_version(floor))
  install.packages(package, repos = c(CRAN = snapshot), lib = library)
  stopifnot(as.character(utils::packageVersion(package, lib.loc = library)) == pinned)
}
for (package in packages) stopifnot(requireNamespace(package, quietly = TRUE))
