test_that("get_first_dose_date works correctly", {
  built_domains <- list(
    EX = data.frame(
      USUBJID = c("S1", "S1", "S2"),
      EXSTDTC = c("2023-01-05T08:00", "2023-01-01T08:00", "2023-01-10"),
      EXDOSE = c("10", "10", "0"), # S2 has 0 dose
      stringsAsFactors = FALSE
    )
  )

  res <- get_first_dose_date(c("S1", "S2", "S3"), built_domains)
  expect_equal(res, c("2023-01-01T08:00", NA_character_, NA_character_))

  res_missing <- get_first_dose_date(c("S1"), NULL)
  expect_equal(res_missing, c(NA_character_))
})

test_that("get_last_dose_date works correctly", {
  built_domains <- list(
    EC = data.frame( # testing fallback to EC
      USUBJID = c("S1", "S1"),
      ECENDTC = c("2023-01-05T08:00", "2023-01-10T08:00"),
      ECDOSE = c("10", "10"),
      stringsAsFactors = FALSE
    )
  )

  res <- get_last_dose_date(c("S1"), built_domains)
  expect_equal(res, c("2023-01-10T08:00"))
})

test_that("get_earliest_informed_consent works with built domains", {
  built_domains <- list(
    DS = data.frame(
      USUBJID = c("S1", "S1", "S2"),
      DSDECOD = c(
        "INFORMED CONSENT OBTAINED",
        "OTHER",
        "INFORMED CONSENT OBTAINED"
      ),
      DSSTDTC = c("2023-01-02", "2023-01-01", "2023-01-05"),
      stringsAsFactors = FALSE
    )
  )

  res <- get_earliest_informed_consent_date(
    c("S1", "S2", "S3"),
    built_domains
  )
  expect_equal(res, c("2023-01-02", "2023-01-05", NA_character_))
})

test_that("get_earliest_informed_consent_date works correctly with raw mode", {
  df_long <- data.frame(
    SubjectKey = c("SUBJ-S1", "SUBJ-S1", "SUBJ-S2"),
    ItemOID = c("DSTERM", "DSSTDAT", "DSTERM"),
    Value = c("Informed Consent Obtained", "2023-01-02", "Other"),
    stringsAsFactors = FALSE
  )

  res <- get_earliest_informed_consent_date(
    c("PROJ-S1", "PROJ-S2"),
    raw_mode = TRUE,
    df_long = df_long
  )
  expect_equal(res, c("2023-01-02", NA_character_))
})

test_that("get_last_participation_date works correctly", {
  built_domains <- list(
    DS = data.frame(
      USUBJID = c("S1"),
      DSSTDTC = c("2023-01-01"),
      stringsAsFactors = FALSE
    ),
    AE = data.frame(
      USUBJID = c("S1", "S2"),
      AEENDTC = c("2023-01-15", "2023-01-10"),
      stringsAsFactors = FALSE
    )
  )

  res <- get_last_participation_date(c("S1", "S2", "S3"), built_domains)
  expect_equal(res, c("2023-01-15", "2023-01-10", NA_character_))
})

test_that("calc_rfendtc works correctly", {
  expect_equal(
    calc_rfendtc(c("2023-01-01", NA), c("5", "10")),
    c("2023-01-05", "")
  )
})
