test_that("build_adam_dataset works correctly", {
  source_data <- list(
    ADSL = data.frame(
      USUBJID = c("S1", "S2"),
      TRT01P = c("A", "B"),
      stringsAsFactors = FALSE
    ),
    VS = data.frame(
      USUBJID = c("S1", "S1", "S2"),
      VSTESTCD = c("SYSBP", "DIABP", "SYSBP"),
      VSSTRESN = c("120", "80", "130"),
      stringsAsFactors = FALSE
    )
  )

  yaml_content <- '
domain: ADVS
key: ["USUBJID", "VSTESTCD"]
data_dependency:
  - adam_variable: "USUBJID"
    sdtm_data: "VS"
columns:
  - name: "STUDYID"
    derivation:
      constant: "TESTSTUDY"
  - name: "TRTP"
    derivation:
      source: "ADSL.TRT01P"
  - name: "AVAL"
    type: "float"
    derivation:
      source: "VS.VSSTRESN"
  - name: "PARAMCD"
    derivation:
      source: "VS.VSTESTCD"
      mapping:
        "SYSBP": "SYSBP"
        "DIABP": "DIABP"
  '
  tmp_file <- tempfile(fileext = ".yaml")
  writeLines(yaml_content, tmp_file)

  res <- build_adam_dataset(tmp_file, source_data)

  expect_equal(nrow(res), 3)
  expect_equal(res$STUDYID, rep("TESTSTUDY", 3))

  trt_mapped <- coalesce(
    source_data$ADSL$TRT01P[match(res$USUBJID, source_data$ADSL$USUBJID)],
    NA_character_
  )
  expect_equal(res$TRTP, trt_mapped)

  expect_true(is.numeric(res$AVAL))
  expect_equal(res$AVAL, as.numeric(source_data$VS$VSSTRESN))

  expect_true("PARAMCD" %in% names(res))
  expect_equal(res$PARAMCD, c("SYSBP", "DIABP", "SYSBP"))
  expect_false(any(is.na(res$PARAMCD)))
  expect_equal(res$VSTESTCD, c("SYSBP", "DIABP", "SYSBP"))

  unlink(tmp_file)
})

test_that("build_adam_dataset invokes function derivations", {
  function_name <- ".issue180_adam_values"
  assign(
    function_name,
    function() c("derived-S1", "derived-S2"),
    envir = .GlobalEnv
  )
  on.exit(rm(list = function_name, envir = .GlobalEnv), add = TRUE)

  source_data <- list(
    ADSL = data.frame(
      USUBJID = c("S1", "S2"),
      stringsAsFactors = FALSE
    )
  )
  yaml_content <- '
domain: ADSL
key: ["USUBJID"]
data_dependency:
  - adam_variable: "USUBJID"
    sdtm_data: "ADSL"
columns:
  - name: "RESULT"
    derivation:
      function_: ".issue180_adam_values"
  '
  tmp_file <- tempfile(fileext = ".yaml")
  on.exit(unlink(tmp_file), add = TRUE)
  writeLines(yaml_content, tmp_file)

  res <- build_adam_dataset(tmp_file, source_data)

  expect_equal(
    res,
    data.frame(
      USUBJID = c("S1", "S2"),
      RESULT = c("derived-S1", "derived-S2"),
      stringsAsFactors = FALSE
    )
  )

  writeLines(
    sub(
      ".issue180_adam_values", ".issue180_missing", yaml_content,
      fixed = TRUE
    ),
    tmp_file
  )
  expect_error(
    build_adam_dataset(tmp_file, source_data),
    "Unable to resolve function: .issue180_missing"
  )
})
