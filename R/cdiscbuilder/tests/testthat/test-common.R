test_that("calculate_study_day handles standard dates correctly", {
  expect_equal(calculate_study_day(c("2023-01-05"), c("2023-01-01")), c(5))
  expect_equal(calculate_study_day(c("2023-01-01"), c("2023-01-01")), c(1))
  expect_equal(calculate_study_day(c("2022-12-31"), c("2023-01-01")), c(-1))
})

test_that("calculate_study_day handles partial and missing dates", {
  expect_equal(
    calculate_study_day(c("2023-01"), c("2023-01-01")),
    c(NA_integer_)
  )
  expect_equal(
    calculate_study_day(c("2023-01-05"), c("2023-01")),
    c(NA_integer_)
  )
  expect_equal(calculate_study_day(c(NA), c("2023-01-01")), c(NA_integer_))
})

test_that("extract_value works correctly", {
  df <- data.frame(
    FormOID = c("F1", "F1", "F2"),
    ItemOID = c("I1", "I2", "I1"),
    Value = c("V1", "V2", "V3"),
    Key1 = c("K1", "K1", "K2"),
    stringsAsFactors = FALSE
  )

  res <- extract_value(
    df,
    form_oids = c("F1"),
    item_oids = c("I1"),
    keys = c("Key1")
  )
  expect_equal(nrow(res), 1)
  expect_equal(res$Value, "V1")
  expect_equal(res$Key1, "K1")

  res2 <- extract_value(df, form_oids = c("F3"))
  expect_equal(nrow(res2), 0)
})

test_that("add_days works correctly", {
  expect_equal(add_days("2023-01-01", 5), "2023-01-06")
  expect_equal(add_days("2023-01-01", -1), "2022-12-31")
  expect_equal(add_days(NA, 5), "")
  expect_equal(add_days("2023-01-01", NA), "")
  expect_equal(add_days("invalid", 5), "")
})

test_that("epoch_of works correctly", {
  expect_equal(
    epoch_of(c("SCREENING", "", "WEEK 1")),
    c("SCREENING", "SCREENING", "TREATMENT")
  )
})

test_that("join_visit works correctly", {
  df <- data.frame(VIS = c("V1", "V2", "V3"), stringsAsFactors = FALSE)
  lookup <- data.frame(
    VISITCD = c("V1", "V2"),
    VISIT = c("Visit 1", "Visit 2"),
    VISITNUM = c(1, 2),
    OFFSET = c(0, 7),
    stringsAsFactors = FALSE
  )

  res <- join_visit(df, lookup, code_col = "VIS")
  expect_equal(res$VISIT, c("Visit 1", "Visit 2", NA))
  expect_equal(res$VISITNUM, c(1, 2, NA))
  expect_equal(res$.OFFSET, c(0, 7, NA))
})

test_that("add_seq works correctly", {
  df <- data.frame(
    USUBJID = c("S1", "S1", "S2"),
    val = 1:3,
    stringsAsFactors = FALSE
  )
  res <- add_seq(df, "SEQ")
  expect_equal(res$SEQ, c("1", "2", "1"))
})
