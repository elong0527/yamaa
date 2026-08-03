#!/usr/bin/env Rscript
# Execute every fixture with the R engine, compare against its expected CSV, and
# compare cell-for-cell against the Python engine's output.
#
#   Rscript yaml/tests/R/run_parity.R
#
# Python output is read from tests/python/emit_json.py, so a difference here is a
# real disagreement between the two implementations rather than a reporting
# artifact.

here <- dirname(normalizePath(sub("^--file=", "",
                grep("^--file=", commandArgs(), value = TRUE))))
Sys.setenv(YAMAA_YAML_DIR = normalizePath(file.path(here, "..", "..")))
source(file.path(here, "validate.R"))
source(file.path(here, "engine.R"))
source(file.path(here, "runner.R"))

d <- load_design()
failures <- 0L

impl <- check_implemented(d)
cat(sprintf("%s registry is implementable\n", if (length(impl)) "FAIL" else "ok  "))
for (e in impl) cat("       ", e, "\n", sep = "")
failures <- failures + length(impl)

python_output <- function(spec) {
  out <- suppressWarnings(system2(
    "uv", c("run", "--quiet", "--with", "pyyaml", "python",
            file.path(here, "..", "python", "emit_json.py"), spec),
    stdout = TRUE, stderr = TRUE))
  if (!is.null(attr(out, "status")) && attr(out, "status") != 0L) {
    return(list(error = paste(out, collapse = "\n")))
  }
  list(rows = jsonlite::fromJSON(paste(out, collapse = ""), simplifyDataFrame = FALSE))
}

for (spec in example_specs()) {
  nm <- basename(dirname(spec))

  got <- tryCatch(run_spec(spec, d), error = function(e) e)
  if (inherits(got, "error")) {
    cat(sprintf("FAIL %s (R execution) %s\n", nm, conditionMessage(got)))
    failures <- failures + 1L
    next
  }

  # 1. R output vs the expected CSV
  exp_path <- Sys.glob(file.path(dirname(spec), "expected", "*.csv"))[1]
  want <- read_csv_chr(exp_path)
  diffs <- character()
  if (length(got) != length(want)) {
    diffs <- c(diffs, sprintf("row count: got %d, expected %d", length(got), length(want)))
  }
  for (i in seq_len(min(length(got), length(want)))) {
    for (col in names(want[[i]])) {
      g <- got[[i]][[col]] %||% ""
      w <- want[[i]][[col]] %||% ""
      if (!identical(as.character(g), as.character(w))) {
        diffs <- c(diffs, sprintf("row %d %s: R gave '%s', expected '%s'", i, col, g, w))
      }
    }
  }

  # 2. R output vs Python output, cell for cell
  py <- python_output(spec)
  if (!is.null(py$error)) {
    diffs <- c(diffs, sprintf("python engine failed: %s", py$error))
  } else {
    if (length(py$rows) != length(got)) {
      diffs <- c(diffs, sprintf("parity row count: R %d, Python %d", length(got), length(py$rows)))
    }
    for (i in seq_len(min(length(got), length(py$rows)))) {
      # Union of both sides: iterating R's names alone hides any column Python
      # emits and R does not.
      for (col in union(names(got[[i]]), names(py$rows[[i]]))) {
        r <- as.character(got[[i]][[col]] %||% "")
        p <- as.character(py$rows[[i]][[col]] %||% "")
        if (!identical(r, p)) {
          diffs <- c(diffs, sprintf("PARITY row %d %s: R '%s' vs Python '%s'", i, col, r, p))
        }
      }
    }
  }

  cat(sprintf("%s %s (%d rows)\n", if (length(diffs)) "FAIL" else "ok  ", nm, length(got)))
  for (x in utils::head(diffs, 12)) cat("       ", x, "\n", sep = "")
  failures <- failures + length(diffs)
}

cat(sprintf("\n%s: %d problem(s)\n", if (failures) "FAILED" else "PASSED", failures))
quit(status = if (failures) 1L else 0L)
