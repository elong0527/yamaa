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

test_that("build_aggregation_sql builds closest marker", {
  sql <- cdiscbuilder:::.build_aggregation_sql(
    "VS.VSSTRESN",
    list(function_ = "closest", target = "TARGET_DATE"),
    NULL,
    "USUBJID"
  )

  expect_equal(
    grepl("CLOSEST:VS.VSSTRESN:TARGET_DATE:", sql, fixed = TRUE),
    TRUE
  )
  expect_equal(grepl("GROUP BY USUBJID", sql, fixed = TRUE), TRUE)
})
