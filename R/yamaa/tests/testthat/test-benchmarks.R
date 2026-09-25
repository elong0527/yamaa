# test-benchmarks.R -- the packaged engine reproduces the clean-room tree's
# benchmark pass counts against goldens: every positive benchmark must
# match its expected artifact, every negative must fail with its expected
# condition, and only the by-design runner_language_mismatch skips remain.

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
  # the only acceptable skips are the Stage-2 function-runtime benchmarks
  bad_skips <- skips[!grepl("runner_language_mismatch", skips, fixed = TRUE)]
  expect_equal(bad_skips, character(0),
    info = "unexpected benchmark skips")
  expect_equal(fails, character(0),
    info = paste0(length(fails), " benchmark failures"))
})
