test_that("get_bmi calculates BMI correctly", {
  expect_equal(get_bmi(180, 75), 75 / (1.8^2))
  expect_equal(get_bmi(160, 50), 50 / (1.6^2))
  expect_true(is.na(get_bmi(NA, 75)))
  expect_true(is.na(get_bmi(180, NA)))
})
