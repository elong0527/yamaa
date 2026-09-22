# test-temporal-datetime.R -- unit tests for datetime_impute / datetime_precision
# (rules/operations/temporal.md REQ-1182..1184).

dt_impute <- yamaa:::op_datetime_impute
dt_prec <- yamaa:::op_datetime_precision
mk_str <- function(v) yamaa:::tv(v, "str")

test_that("datetime_impute passes a complete datetime through (REQ-1182)", {
  r <- dt_impute(mk_str("2025-01-08T09:00:00"), "first")
  expect_equal(r$t, "datetime")
  expect_equal(r$v, "2025-01-08T09:00:00")
  expect_equal(attr(r, "precision"), "second")
})

test_that("datetime_impute completes a date at the declared day edge", {
  r_first <- dt_impute(mk_str("2025-01-08"), "first")
  expect_equal(r_first$v, "2025-01-08T00:00:00")
  expect_equal(attr(r_first, "precision"), "day")
  r_last <- dt_impute(mk_str("2025-01-08"), "last")
  expect_equal(r_last$v, "2025-01-08T23:59:59")
  expect_equal(attr(r_last, "precision"), "day")
})

test_that("datetime_impute rejects a bad time_rule before reading data", {
  expect_error(dt_impute(mk_str("2025-01-08"), "noon"),
    class = "yamaa_error")
})

test_that("datetime_impute fails invalid_datetime_text on truncated sources", {
  # a source truncated before its day gets no second imputation policy
  for (s in c("2025-01", "2025", "not-a-date", "2025-13-40")) {
    e <- tryCatch({ dt_impute(mk_str(s), "first"); NULL },
      yamaa_error = function(e) e)
    expect_false(is.null(e), info = s)
    expect_equal(attr(e, "yamaa_condition"), "invalid_datetime_text")
  }
})

test_that("datetime_impute honors missing/invalid handlers", {
  r <- dt_impute(mk_str(c(NA, "2025-01")), "first",
    missing_h = "MISSING", invalid_h = "INVALID")
  expect_equal(r$v, c("MISSING", "INVALID"))
})

test_that("datetime_precision reports S for time, D for imputed day", {
  expect_equal(dt_prec(mk_str("2025-01-08T09:00:00"), NA, NULL)$v, "S")
  expect_equal(dt_prec(mk_str("2025-01-08"), NA, NULL)$v, "D")
})

test_that("datetime_precision reads a datetime value's completed precision", {
  imputed <- dt_impute(mk_str("2025-01-08"), "first")
  expect_equal(dt_prec(imputed, NA, NULL)$v, "D")
  passthrough <- dt_impute(mk_str("2025-01-08T09:00:00"), "first")
  expect_equal(dt_prec(passthrough, NA, NULL)$v, "S")
})

test_that("datetime_precision fails invalid_datetime_text without a handler", {
  e <- tryCatch({ dt_prec(mk_str("2025-01"), NA, NULL); NULL },
    yamaa_error = function(e) e)
  expect_equal(attr(e, "yamaa_condition"), "invalid_datetime_text")
})
