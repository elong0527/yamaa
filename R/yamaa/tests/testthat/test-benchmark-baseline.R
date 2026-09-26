# test-benchmark-baseline.R -- unit tests for classify_benchmark_outcomes()
# (R/yamaa/tests/testthat/helper-benchmark.R). Synthetic fail/skip
# vectors only; the real baseline constants ship as the function's default
# arguments, so these tests pin the classification logic, not the content.

test_that("classify_benchmark_outcomes tolerates baseline failures and skips", {
  cls <- classify_benchmark_outcomes(
    fails = c("bench-a: cell mismatch", "bench-b: exploded: with colon"),
    skips = c("bench-c: no spec.yaml"),
    known_failures = c("bench-a", "bench-b"),
    known_skips = c("bench-c"))
  expect_equal(cls$new_fails, character(0))
  expect_equal(cls$new_skips, character(0))
  expect_equal(cls$recovered_fails, character(0))
  expect_equal(cls$recovered_skips, character(0))
})

test_that("classify_benchmark_outcomes flags new failures and skips", {
  cls <- classify_benchmark_outcomes(
    fails = c("bench-a: old gap", "bench-new: regression"),
    skips = c("bench-c: no spec.yaml", "bench-other: unexpected skip"),
    known_failures = c("bench-a"),
    known_skips = c("bench-c"))
  expect_equal(cls$new_fails, "bench-new: regression")
  expect_equal(cls$new_skips, "bench-other: unexpected skip")
})

test_that("classify_benchmark_outcomes ignores runner_language_mismatch skips", {
  cls <- classify_benchmark_outcomes(
    fails = character(0),
    skips = c("stage2-bench: runner_language_mismatch in stage 2"),
    known_failures = character(0),
    known_skips = character(0))
  expect_equal(cls$new_skips, character(0))
})

test_that("classify_benchmark_outcomes reports recovered baseline items", {
  cls <- classify_benchmark_outcomes(
    fails = c("bench-a: still broken"),
    skips = character(0),
    known_failures = c("bench-a", "bench-fixed"),
    known_skips = c("bench-unskipped"))
  expect_equal(cls$recovered_fails, "bench-fixed")
  expect_equal(cls$recovered_skips, "bench-unskipped")
})

test_that("classify_benchmark_outcomes handles entries without a detail suffix", {
  cls <- classify_benchmark_outcomes(
    fails = "bench-a",
    skips = character(0),
    known_failures = c("bench-a"),
    known_skips = character(0))
  expect_equal(cls$new_fails, character(0))
})

test_that("default baseline wires through the real parity-gap constants", {
  cls <- classify_benchmark_outcomes(
    fails = "adam-advs-windows: synthetic detail",
    skips = "sdtm-dm-race-ethnicity: synthetic detail")
  expect_equal(cls$new_fails, character(0))
  expect_equal(cls$new_skips, character(0))
})
