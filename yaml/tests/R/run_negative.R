#!/usr/bin/env Rscript
# Every negative fixture must be rejected by the R validator too, with the same
# expected message. Divergence here is a real conformance gap between the two
# implementations, which is exactly what this file exists to expose.

here <- dirname(normalizePath(sub("^--file=", "",
                grep("^--file=", commandArgs(), value = TRUE))))
Sys.setenv(YAMAA_YAML_DIR = normalizePath(file.path(here, "..", "..")))
source(file.path(here, "validate.R"))

neg <- sort(Sys.glob(file.path(here, "..", "negative", "*")))
neg <- neg[file.exists(file.path(neg, "spec.yaml"))]
d <- load_design()
failures <- 0L

for (case in neg) {
  nm <- basename(case)
  want <- trimws(readLines(file.path(case, "expect.txt"), warn = FALSE)[1])
  errs <- tryCatch(validate_spec(file.path(case, "spec.yaml"), d),
                   error = function(e) paste("VALIDATOR ERROR:", conditionMessage(e)))
  if (!length(errs)) {
    cat(sprintf("FAIL %s: accepted, expected rejection\n", nm)); failures <- failures + 1L
  } else if (!any(grepl(want, errs, fixed = TRUE))) {
    cat(sprintf("FAIL %s: rejected for the wrong reason\n", nm))
    cat(sprintf("       wanted substring: %s\n", want))
    for (e in utils::head(errs, 3)) cat("       got: ", e, "\n", sep = "")
    failures <- failures + 1L
  } else {
    cat(sprintf("ok   %s\n", nm))
  }
}
cat(sprintf("\n%s: %d of %d negative cases wrong\n",
            if (failures) "FAILED" else "PASSED", failures, length(neg)))
quit(status = if (failures) 1L else 0L)
