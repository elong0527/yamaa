test_that("topological_sort works correctly", {
  config <- list(
    DM = list(columns = list(COL1 = list(source = "DM.USUBJID"))),
    AE = list(columns = list(COL1 = list(source = "DM.USUBJID"))),
    SUPPAE = list(columns = list(COL1 = list(source = "AE.AETERM")))
  )

  order <- topological_sort(config)

  expect_true(which(order == "DM") < which(order == "AE"))
  expect_true(which(order == "AE") < which(order == "SUPPAE"))
})

test_that("topological_sort detects circular dependencies", {
  config <- list(
    A = list(columns = list(COL1 = list(source = "B.ID"))),
    B = list(columns = list(COL1 = list(source = "A.ID")))
  )
  expect_error(topological_sort(config), "Circular dependency")
})

test_that("process_domain handles standard domains", {
  df_long <- data.frame(
    StudyOID = c("S1", "S1"),
    SubjectKey = c("SUBJ1", "SUBJ1"),
    ItemGroupRepeatKey = c("1", "1"),
    StudyEventOID = c("SE1", "SE1"),
    FormOID = c("F1", "F1"),
    ItemOID = c("I1", "I2"),
    Value = c("Val1", "Val2"),
    stringsAsFactors = FALSE
  )

  sources <- list(
    list(
      formoid = c("F1"),
      columns = list(
        DOMAIN = list(literal = "DM"),
        USUBJID = list(source = "SubjectKey"),
        VAR1 = list(source = "I1"),
        VAR2 = list(source = "I2", type = "str")
      )
    )
  )

  res <- process_domain(
    "DM",
    sources,
    df_long,
    default_keys = c(
      "StudyOID",
      "SubjectKey",
      "ItemGroupRepeatKey",
      "StudyEventOID"
    )
  )

  expect_equal(nrow(res), 1)
  expect_equal(res$DOMAIN, "DM")
  expect_equal(res$USUBJID, "SUBJ1")
  expect_equal(res$VAR1, "Val1")
  expect_equal(res$VAR2, "Val2")
})

test_that("process_domain handles FINDINGS domains", {
  df_long <- data.frame(
    StudyOID = c("S1", "S1"),
    SubjectKey = c("SUBJ1", "SUBJ1"),
    ItemGroupRepeatKey = c("1", "1"),
    StudyEventOID = c("SE1", "SE1"),
    FormOID = c("F1", "F1"),
    ItemOID = c("TEST", "RES"),
    Value = c("WEIGHT", "75"),
    stringsAsFactors = FALSE
  )

  sources <- list(
    type = "FINDINGS",
    columns = list(
      list(
        formoid = c("F1"),
        VSTEST = list(source = "TEST"),
        VSORRES = list(source = "RES")
      )
    ),
    definitions = list(
      DOMAIN = list(literal = "VS")
    )
  )

  res <- process_domain(
    "VS",
    sources,
    df_long,
    default_keys = c(
      "StudyOID",
      "SubjectKey",
      "ItemGroupRepeatKey",
      "StudyEventOID"
    )
  )

  expect_equal(nrow(res), 1)
  expect_equal(res$DOMAIN, "VS")
  expect_equal(res$VSTEST, "WEIGHT")
  expect_equal(res$VSORRES, "75")
})
