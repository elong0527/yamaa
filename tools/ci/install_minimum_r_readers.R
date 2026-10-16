# Install exact declared reader floors after their ordinary dependencies.
# Run qualification in a fresh process so replaced namespaces cannot remain loaded.
arguments <- commandArgs(trailingOnly = TRUE)
description <- if (length(arguments)) arguments[[1L]] else "R/yamaa/DESCRIPTION"
suggests <- strsplit(read.dcf(description)[1L, "Suggests"], ",", fixed = TRUE)[[1L]]
declarations <- trimws(suggests)
stopifnot(all(grepl("^[a-zA-Z][a-zA-Z0-9.]* \\(>= [0-9.]+\\)$", declarations)))
packages <- sub(" .*", "", declarations)
versions <- sub(".*>= ([0-9.]+)\\)$", "\\1", declarations)
library <- .libPaths()[[1L]]
Sys.setenv(LIBARROW_BINARY = "true", ARROW_USE_PKG_CONFIG = "false", NOT_CRAN = "true")
for (i in seq_along(packages)) {
  package <- packages[[i]]; version <- versions[[i]]
  archive <- paste0(package, "_", version, ".tar.gz")
  urls <- c(paste0("https://cran.r-project.org/src/contrib/", archive),
            paste0("https://cran.r-project.org/src/contrib/Archive/", package, "/", archive))
  target <- tempfile(fileext = ".tar.gz")
  downloaded <- FALSE
  for (url in urls) {
    downloaded <- tryCatch({
      download.file(url, target, mode = "wb", quiet = TRUE) == 0L
    }, error = function(error) FALSE)
    if (downloaded) break
  }
  stopifnot(downloaded)
  install.packages(target, repos = NULL, type = "source", lib = library)
  unlink(target)
  stopifnot(as.character(utils::packageVersion(package, lib.loc = library)) == version)
}
for (package in packages) stopifnot(requireNamespace(package, quietly = TRUE))
