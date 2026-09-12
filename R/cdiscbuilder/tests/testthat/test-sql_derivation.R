test_that("apply_sql_cut works correctly", {
  # testing the unexported function
  series <- c("1", "5", "10", "NA")
  cuts <- list(
    "<= 3" = "LOW",
    "> 3 and <= 8" = "MED",
    "> 8" = "HIGH"
  )

  res <- cdiscbuilder:::.apply_sql_cut(series, cuts)
  expect_equal(res, c("LOW", "MED", "HIGH", NA_character_))
})

test_that("execute_closest works correctly", {
  target_df <- data.frame(
    USUBJID = c("S1", "S1"),
    TARGET_DATE = c("2023-01-05", "2023-01-10"),
    stringsAsFactors = FALSE
  )
  source_data <- list(
    VS = data.frame(
      USUBJID = c("S1", "S1", "S1"),
      VSDTC = c("2023-01-04", "2023-01-06", "2023-01-11"),
      VSSTRESN = c("120", "125", "130"),
      stringsAsFactors = FALSE
    )
  )

  res <- cdiscbuilder:::.execute_closest(
    "CLOSEST:VS.VSSTRESN:TARGET_DATE",
    c("USUBJID"),
    target_df,
    source_data
  )
  expect_equal(res, c("120", "130"))
})

test_that("build_aggregation_sql returns a dispatchable closest marker", {
  sql <- cdiscbuilder:::.build_aggregation_sql(
    "VS.VSSTRESN",
    list(function_ = "closest", target = "TARGET_DATE"),
    NULL,
    "USUBJID"
  )

  # .execute_sql() routes to .execute_closest() only on the bare marker.
  expect_equal(sql, "CLOSEST:VS.VSSTRESN:TARGET_DATE:")
  expect_equal(startsWith(sql, "CLOSEST:"), TRUE)
})

test_that("execute_sql preserves numeric literals", {
  target_df <- data.frame(
    USUBJID = c("S1", "S2"),
    stringsAsFactors = FALSE
  )

  res <- cdiscbuilder:::.execute_sql(
    "SELECT USUBJID, 1.5 as result FROM merged",
    "USUBJID",
    target_df,
    list()
  )

  expect_equal(res, c(1.5, 1.5))
})

test_that("build_aggregation_sql rejects first and last", {
  for (func in c("first", "last")) {
    expect_error(
      cdiscbuilder:::.build_aggregation_sql(
        "VS.VSSTRESN",
        list(function_ = func),
        NULL,
        "USUBJID"
      ),
      paste0("Unsupported aggregation function: ", func),
      fixed = TRUE
    )
  }
})

test_that("execute_closest rejects invalid filters", {
  target_df <- data.frame(
    USUBJID = "S1",
    TARGET_DATE = "2023-01-05",
    stringsAsFactors = FALSE
  )
  source_data <- list(
    VS = data.frame(
      USUBJID = "S1",
      VSDTC = "2023-01-04",
      VSSTRESN = "120",
      VSTESTCD = "SYSBP",
      stringsAsFactors = FALSE
    )
  )

  for (filter_expr in c("VS.VSTESTCD ==", "VS.MISSING == 'SYSBP'")) {
    expect_error(
      cdiscbuilder:::.execute_closest(
        paste0("CLOSEST:VS.VSSTRESN:TARGET_DATE:", filter_expr),
        "USUBJID",
        target_df,
        source_data
      ),
      "Filter failed:",
      fixed = TRUE
    )
  }
})
