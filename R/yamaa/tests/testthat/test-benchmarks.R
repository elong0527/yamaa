# test-benchmarks.R -- the packaged engine reproduces the clean-room tree's
# benchmark pass counts against goldens: every positive benchmark must
# match its expected artifact, every negative must fail with its expected
# condition, and only the by-design runner_language_mismatch skips remain.
#
# Known clean-room parity gaps are an explicit tolerated baseline below.
# The suite is green when the packaged engine is no worse than the tree
# and fails only on NEW failures or skips -- a red run always means the
# packaged engine diverged from the clean-room tree. Parity findings live
# in the clean-room tree's FINDINGS.md (daily entries); when a baseline
# gap closes, delete its name from the baseline so the suite keeps
# shrinking toward zero.

test_that("packaged engine matches goldens across the benchmark corpus", {
  bdir <- yamaa_benchmarks_root()
  # the benchmarks directory must exist: silently skipping here would let
  # this flagship parity test pass with zero benchmarks executed, so fail
  # loudly instead of skipping when the corpus is absent.
  expect_true(dir.exists(bdir),
    info = paste0("benchmarks directory not found: ", bdir))
  man <- yaml::yaml.load_file(file.path(bdir, "execution-manifest.yaml"),
    handlers = yamaa_handlers())$examples
  fails <- character(0)
  skips <- character(0)
  for (nm in sort(names(man))) {
    r <- run_one_benchmark(nm, dirname(bdir), man[[nm]])
    if (r$status == "FAIL")
      fails <- c(fails, paste0(nm, ": ", r$detail))
    if (r$status == "SKIP")
      skips <- c(skips, paste0(nm, ": ", r$detail))
  }
  fail_names <- sub(":.*$", "", fails)
  # Tolerated baseline: pre-existing clean-room parity gaps, identical in
  # the unpackaged tree (FINDINGS.md, daily parity entries, 284/292).
  known_failures <- c(
    "adam-adae-partial-dates",
    "adam-advs-windows",
    "negative-output-missing-key",
    "negative-str-contains-bool-result",
    "schema-text-mapping-unmapped",
    "sdtm-tr-tumor-measurements",
    "sdtm-vs-collected-form"
  )
  known_skips <- c(
    "sdtm-dm-race-ethnicity"  # no spec.yaml in the corpus
  )
  # the only acceptable skips are the Stage-2 function-runtime benchmarks
  # (runner_language_mismatch) plus the known-skip baseline
  bad_skips <- skips[!grepl("runner_language_mismatch", skips, fixed = TRUE)]
  new_skips <- bad_skips[!sub(":.*$", "", bad_skips) %in% known_skips]
  expect_equal(new_skips, character(0),
    info = "unexpected benchmark skips")
  new_fails <- fails[!fail_names %in% known_failures]
  expect_equal(new_fails, character(0),
    info = paste0(length(new_fails), " new benchmark failures"))
  # informational only: a baseline gap that now passes should be removed
  # from the baseline so the suite keeps shrinking toward zero
  recovered <- known_failures[!known_failures %in% fail_names]
  if (length(recovered) > 0)
    message("now passing; remove from known_failures: ",
      paste(recovered, collapse = ", "))
  unskipped <- known_skips[!known_skips %in% sub(":.*$", "", skips)]
  if (length(unskipped) > 0)
    message("no longer skipped; remove from known_skips: ",
      paste(unskipped, collapse = ", "))
})
