test_that("CSV ingestion preserves typeless text and empty provenance", {
  working_dir <- tempfile("sdtm-csv-ingestion-")
  config_dir <- file.path(working_dir, "config")
  output_dir <- file.path(working_dir, "output")
  dir.create(config_dir, recursive = TRUE)
  on.exit(unlink(working_dir, recursive = TRUE))

  input_csv <- file.path(working_dir, "input.csv")
  writeLines(
    c(
      paste(
        "StudyOID,SubjectKey,ItemGroupRepeatKey,StudyEventOID,FormOID",
        "Site,ItemOID,Value",
        sep = ","
      ),
      "S1,001,1,E1,F1,007,I1,",
      'S1,001,1,E1,F1,007,I2,""'
    ),
    input_csv,
    useBytes = TRUE
  )
  writeLines(
    c(
      "DM:",
      "  formoid: F1",
      paste(
        "  keys: [StudyOID, SubjectKey, ItemGroupRepeatKey,",
        "StudyEventOID, Site]"
      ),
      "  columns:",
      "    SUBJECT: {source: SubjectKey, type: str}",
      "    SITE: {source: Site, type: str}",
      "    BARE: {source: I1, type: str}",
      "    QUOTED: {source: I2, type: str}"
    ),
    file.path(config_dir, "dm.yaml"),
    useBytes = TRUE
  )

  datasets <- create_sdtm_datasets(config_dir, input_csv, output_dir)

  expect_true(all(datasets$DM$SUBJECT == "001"))
  expect_true(all(datasets$DM$SITE == "007"))
  expect_true(all(is.na(datasets$DM$BARE)))
  expect_true(all(datasets$DM$QUOTED == ""))
})
