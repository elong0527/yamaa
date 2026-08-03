#!/usr/bin/env Rscript
# Validate every example specification with the R implementation.
#
# Run from the repository root:
#   Rscript yaml/tests/R/run_tests.R

here <- dirname(normalizePath(sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE))))
Sys.setenv(YAMAA_YAML_DIR = normalizePath(file.path(here, "..", "..")))
source(file.path(here, "validate.R"))

d <- load_design()
failures <- 0L

reg <- check_registries(d)
cat(sprintf("%s registries\n", if (length(reg)) "FAIL" else "ok  "))
for (e in reg) cat("       ", e, "\n", sep = "")
failures <- failures + length(reg)

for (spec in example_specs()) {
  nm <- basename(dirname(spec))
  errs <- validate_spec(spec, d)
  cat(sprintf("%s %s\n", if (length(errs)) "FAIL" else "ok  ", nm))
  for (e in errs) cat("       ", e, "\n", sep = "")
  failures <- failures + length(errs)
}

cat(sprintf("\n%s: %d problem(s)\n", if (failures) "FAILED" else "PASSED", failures))
quit(status = if (failures) 1L else 0L)
