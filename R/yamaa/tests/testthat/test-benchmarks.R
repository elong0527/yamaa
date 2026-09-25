# test-benchmarks.R -- the packaged engine reproduces the clean-room tree's
# benchmark pass counts against goldens: every positive benchmark must
# match its expected artifact, every negative must fail with its expected
# condition, and only the by-design runner_language_mismatch skips remain.
#
# Known clean-room parity gaps are an explicit tolerated baseline in
# helper-benchmark.R (benchmark_known_failures / benchmark_known_skips,
# classified by classify_benchmark_outcomes(), unit-tested in
# test-benchmark-baseline.R). The suite is green when the packaged engine
# is no worse than the tree and fails only on NEW failures or skips -- a
# red run always means the packaged engine diverged from the clean-room
# tree. Parity findings live in the clean-room tree's FINDINGS.md (daily
# entries); when a baseline gap closes, delete its name from the baseline
# so the suite keeps shrinking toward zero.

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
  cls <- classify_benchmark_outcomes(fails, skips)
  expect_equal(cls$new_skips, character(0),
    info = "unexpected benchmark skips")
  expect_equal(cls$new_fails, character(0),
    info = paste0(length(cls$new_fails), " new benchmark failures"))
  # informational only: a baseline gap that now passes should be removed
  # from the baseline so the suite keeps shrinking toward zero
  if (length(cls$recovered_fails) > 0)
    message("now passing; remove from benchmark_known_failures: ",
      paste(cls$recovered_fails, collapse = ", "))
  if (length(cls$recovered_skips) > 0)
    message("no longer skipped; remove from benchmark_known_skips: ",
      paste(cls$recovered_skips, collapse = ", "))
})
