test_that("an empty delimited field is missing whether bare or quoted", {
  input_csv <- tempfile("delimited-empty-missing-")
  on.exit(unlink(input_csv))
  writeLines(
    c(
      "ID,BARE,QUOTED",
      "1,,\"\""
    ),
    input_csv,
    useBytes = TRUE
  )

  datasets <- cdiscbuilder:::.read_delimited_source(input_csv)

  expect_true(is.na(datasets$BARE))
  expect_true(is.na(datasets$QUOTED))
})

test_that("blank fields in the string-handler fixtures are missing", {
  root <- normalizePath(
    file.path("..", "..", "..", ".."),
    mustWork = FALSE
  )
  handlers_input <- file.path(
    root, "yaml", "examples", "adam-adae-string-handlers", "input", "ae.csv"
  )
  comment_input <- file.path(
    root, "yaml", "examples", "adam-adsl-investigator-comment", "input", "dm.csv"
  )
  skip_if_not(
    file.exists(handlers_input) && file.exists(comment_input),
    "the example fixtures are not beside the package"
  )

  handlers <- cdiscbuilder:::.read_delimited_source(handlers_input)
  comment <- cdiscbuilder:::.read_delimited_source(comment_input)

  expect_true(all(is.na(handlers$AESPID[handlers$AESEQ == "2"])))
  expect_equal(
    comment$USUBJID[is.na(comment$COMMENT)],
    c("CTX-04", "CTX-05")
  )
})
