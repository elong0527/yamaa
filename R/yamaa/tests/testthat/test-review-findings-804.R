# Regression tests for the PR #804 review findings:
# sidecar names, null-valued mapping entries, the tagged int -2147483648,
# and REQ-0006 finite normalization of numeric functions.

test_that("sidecars publish as warning_log and verification_log", {
  d <- tempfile("sidecar-")
  dir.create(d)
  dir.create(file.path(d, "input"))
  write.csv(data.frame(USUBJID = "S1", AGE = 150),
    file.path(d, "input", "dm.csv"), row.names = FALSE)
  spec <- c(
    'schema_version: "1.0"',
    'domain: ADSL',
    'keys: [USUBJID]',
    'input:',
    '  DM: input/dm.csv',
    'output:',
    '  path: adsl.csv',
    '  columns: [USUBJID, AGE]',
    '  warning_log: adsl-warnings.csv',
    '  verification_log: adsl-checks.csv',
    'columns:',
    '  - name: USUBJID',
    '    type: str',
    '    derivation: DM.USUBJID',
    '  - name: AGE',
    '    type: int',
    '    derivation: DM.AGE',
    '    verifications:',
    '      - range:',
    '          min: 18',
    '          max: 100',
    '          severity: warning',
    'verifications:',
    '  - unique:',
    '      columns: [USUBJID]')
  writeLines(spec, file.path(d, "spec.yaml"))
  yamaa::run_spec(file.path(d, "spec.yaml"), file.path(d, "adsl.csv"))
  # the declared sidecar names exist and carry content ...
  expect_true(file.exists(file.path(d, "adsl-warnings.csv")))
  expect_true(file.exists(file.path(d, "adsl-checks.csv")))
  expect_gt(nrow(utils::read.csv(file.path(d, "adsl-warnings.csv"))), 0)
  expect_gt(nrow(utils::read.csv(file.path(d, "adsl-checks.csv"))), 0)
  # ... and nothing is published under the retired names
  outs <- list.files(d)
  expect_false(any(grepl("violation_log|verification_report", outs)))
})

test_that("mapping: a null-valued dict entry maps to typed missing, not unmapped (REQ-1110)", {
  dict <- list(a = 1, b = NULL)
  ctx_ab <- list(n = 2, col = list(SRC = yamaa:::tv(c("a", "b"), "str")))
  out <- yamaa:::eval_mapping(list(source = "SRC", dict = dict), ctx_ab)
  expect_equal(out$t, "int")
  expect_equal(out$v, c(1, NA_real_))
  # a matched null is mapped: it must not consult the missing: policy ...
  ctx3 <- list(n = 3, col = list(SRC = yamaa:::tv(c("a", "b", "c"), "str")))
  out_m <- yamaa:::eval_mapping(
    list(source = "SRC", dict = dict, missing = "7"), ctx3)
  expect_equal(out_m$v, c(1, NA_real_, 7))
  # ... and must not trigger strict: true
  out_s <- yamaa:::eval_mapping(
    list(source = "SRC", dict = dict, strict = TRUE), ctx_ab)
  expect_equal(out_s$v, c(1, NA_real_))
})

test_that("compute: -2147483648 parses as one int32 literal (REQ-0434)", {
  resolve <- list(resolve = function(nm) stop("unbound: ", nm), n = 1)
  ev <- function(x) yamaa:::eval_compute(yamaa:::parse_compute_text(x), resolve)
  out <- ev("-2147483648")
  expect_equal(out$t, "int")
  # the actual value, not NA: R's NA_integer_ IS the -2^31 bit pattern
  expect_identical(out$v, -2147483648)
  expect_false(is.na(out$v))
  # ... and the aggregate grammar folds the sign the same way
  ra <- list(n = 1, resolve = function(nm) stop("unbound: ", nm))
  aout <- yamaa:::eval_agg_node(yamaa:::parse_aggregate_text("-2147483648"), ra)
  expect_equal(aout$t, "int")
  expect_identical(aout$v, -2147483648)
})

test_that("tagged int -2147483648 propagates through arithmetic, text, and conversion", {
  resolve <- list(resolve = function(nm) stop("unbound: ", nm), n = 1)
  ev <- function(x) yamaa:::eval_compute(yamaa:::parse_compute_text(x), resolve)
  # arithmetic keeps the value and the tag
  expect_identical(ev("-2147483648 + 0")$v, -2147483648)
  expect_equal(ev("-2147483648 + 0")$t, "int")
  expect_identical(ev("-2147483648 - -1")$v, -2147483647)
  # negating it overflows int32 loudly instead of wrapping to NA
  expect_error(ev("-(-2147483648)"), "\\[integer_overflow\\]")
  # a bare 2147483648 literal is out of int32 range, also loudly
  expect_error(ev("2147483648"), "\\[integer_overflow\\]")
  # ABS(-2^31) has no int32 representation: loud, not silent
  expect_error(ev("ABS(-2147483648)"), "\\[integer_overflow\\]")
  # canonical text renders the full value, never scientific notation
  expect_equal(yamaa:::to_canon_text(yamaa:::tv(-2147483648, "int")), "-2147483648")
  expect_equal(yamaa:::to_canon_text(yamaa:::tv(10000000, "int")), "10000000")
  # str -> int conversion accepts the boundary value
  expect_identical(
    yamaa:::convert_tv(yamaa:::tv("-2147483648", "str"), "int")$v, -2147483648)
})

test_that("REQ-0006: every float-producing numeric function normalizes non-finite to missing", {
  resolve <- list(resolve = function(nm) stop("unbound: ", nm), n = 1)
  ev <- function(x) yamaa:::eval_compute(yamaa:::parse_compute_text(x), resolve)
  fns <- c("EXP(1000)", "POWER(10, 400)", "SQRT(EXP(1000))",
    "ABS(EXP(1000))", "CEIL(EXP(1000))", "FLOOR(EXP(1000))",
    "TRUNC(EXP(1000))", "LN(EXP(1000))", "MOD(1, EXP(1000))",
    "GREATEST(EXP(1000), EXP(1000))", "LEAST(EXP(1000), EXP(1000))",
    "NULLIF(EXP(1000), 1)", "COALESCE(EXP(1000), EXP(1000))",
    "1e999", "EXP(1000) + 1")
  for (fn in fns) {
    out <- ev(fn)
    expect_equal(out$t, "float", info = fn)
    expect_true(is.na(out$v), info = fn)
  }
  # ROUND is a project function, not a compute function: exercise it directly
  rout <- yamaa:::round_half_away(yamaa:::tv(c(Inf, 1.5), "float"), 0)
  expect_true(is.na(rout$v[1]))
  expect_equal(rout$v[2], 2)
  # finite results still pass through untouched
  expect_equal(ev("CEIL(1.5)")$v, 2)
  expect_equal(ev("GREATEST(1, 2)")$v, 2)
})
