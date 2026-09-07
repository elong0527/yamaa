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

test_that("create_sdtm_datasets preserves ODM record identity keys", {
  config_dir <- tempfile("sdtm-config-")
  output_dir <- tempfile("sdtm-output-")
  dir.create(config_dir)
  on.exit(unlink(c(config_dir, output_dir), recursive = TRUE), add = TRUE)

  yaml::write_yaml(
    list(VS = list(list(
      columns = list(VSORRES = list(source = "RES"))
    ))),
    file.path(config_dir, "vs.yaml")
  )
  input_csv <- file.path(config_dir, "input.csv")
  identity_keys <- c("StudyEventRepeatKey", "FormOID", "ItemGroupOID")
  for (identity_key in identity_keys) {
    input <- data.frame(
      StudyOID = rep("S1", 3),
      SubjectKey = rep("SUBJ1", 3),
      ItemGroupRepeatKey = rep("1", 3),
      StudyEventOID = rep("SE1", 3),
      StudyEventRepeatKey = rep("1", 3),
      FormOID = rep("F1", 3),
      ItemGroupOID = rep("IG1", 3),
      ItemOID = rep("RES", 3),
      Value = c("100", "200", "300")
    )
    input[[identity_key]] <- c("1", "2", "3")
    write.csv(input, input_csv, row.names = FALSE)

    result <- create_sdtm_datasets(config_dir, input_csv, output_dir)

    expect_equal(
      sort(result$VS$VSORRES),
      c("100", "200", "300"),
      info = identity_key
    )
  }
})

test_that("process_domain rejects duplicate ItemOID values within a full key", {
  df_long <- data.frame(
    StudyOID = c("S1", "S1"),
    SubjectKey = c("SUBJ1", "SUBJ1"),
    ItemGroupRepeatKey = c("1", "1"),
    StudyEventOID = c("SE1", "SE1"),
    StudyEventRepeatKey = c("1", "1"),
    FormOID = c("F1", "F1"),
    ItemOID = c("RES", "RES"),
    Value = c("100", "200")
  )
  default_keys <- c(
    "StudyOID", "SubjectKey", "ItemGroupRepeatKey", "StudyEventOID",
    "StudyEventRepeatKey"
  )
  standard <- list(list(
    formoid = "F1",
    columns = list(VSORRES = list(source = "RES"))
  ))
  findings <- list(
    type = "FINDINGS",
    columns = list(list(
      formoid = "F1",
      VSORRES = list(source = "RES")
    ))
  )

  expect_error(
    process_domain("VS", standard, df_long, default_keys),
    "Duplicate ItemOID values for one complete key",
    fixed = TRUE
  )
  expect_error(
    process_domain("VS", findings, df_long, default_keys),
    "Duplicate ItemOID values for one complete key",
    fixed = TRUE
  )
})

test_that("cross-domain mappings require every declared lookup key", {
  pivoted <- data.frame(
    USUBJID = "S1",
    VISITNUM = 1,
    stringsAsFactors = FALSE
  )
  col_cfg <- list(
    source = "REF.VALUE",
    merge_on = c("USUBJID", "VISITNUM")
  )

  expect_error(
    .apply_column_mapping(
      pivoted,
      "RESULT",
      col_cfg,
      list(REF = data.frame(
        USUBJID = "S1",
        VALUE = "matched",
        stringsAsFactors = FALSE
      )),
      pivoted
    ),
    "Lookup keys not found in REF: VISITNUM",
    fixed = TRUE
  )

  target_without_visit <- pivoted["USUBJID"]
  expect_error(
    .apply_column_mapping(
      target_without_visit,
      "RESULT",
      col_cfg,
      list(REF = data.frame(
        USUBJID = "S1",
        VISITNUM = 1,
        VALUE = "matched",
        stringsAsFactors = FALSE
      )),
      target_without_visit
    ),
    "Lookup keys not found in target dataset: VISITNUM",
    fixed = TRUE
  )
})

test_that("cross-domain mappings reject ambiguous right-side matches", {
  target <- data.frame(USUBJID = "S1", stringsAsFactors = FALSE)
  reference <- data.frame(
    USUBJID = c("S1", "S1"),
    VALUE = c(20, 10),
    stringsAsFactors = FALSE
  )

  for (row_order in list(1:2, 2:1)) {
    expect_error(
      .apply_column_mapping(
        target,
        "RESULT",
        list(source = "REF.VALUE", merge_on = "USUBJID"),
        list(REF = reference[row_order, ]),
        target
      ),
      "Lookup source REF has multiple matches for keys: USUBJID",
      fixed = TRUE
    )
  }

  incomplete_target <- data.frame(USUBJID = c("S1", NA), stringsAsFactors = FALSE)
  incomplete_reference <- data.frame(
    USUBJID = c("S1", NA, NA),
    VALUE = c("matched", "unused-a", "unused-b"),
    stringsAsFactors = FALSE
  )
  expect_equal(
    .apply_column_mapping(
      incomplete_target,
      "RESULT",
      list(source = "REF.VALUE", merge_on = "USUBJID"),
      list(REF = incomplete_reference),
      incomplete_target
    ),
    c("matched", NA_character_)
  )
})
