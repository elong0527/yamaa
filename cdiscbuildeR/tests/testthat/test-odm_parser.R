test_that("extract_metadata_summary works correctly", {
  df_long <- data.frame(
    FormOID = c("F1", "F1", "F2"),
    ItemGroupOID = c("IG1", "IG1", "IG2"),
    ItemOID = c("I1", "I1", "I2"),
    ItemName = c("Item 1", "Item 1", "Item 2"),
    Question = c("Q1", "Q1", "Q2"),
    Value = c("A", "B", NA),
    stringsAsFactors = FALSE
  )

  res <- extract_metadata_summary(df_long)
  expect_equal(nrow(res), 2)
  expect_equal(res$SampleValues, c("['A', 'B']", "[]"))

  res_empty <- extract_metadata_summary(NULL)
  expect_equal(nrow(res_empty), 0)
})

test_that("parse_odm_to_long_df handles empty or invalid xml safely", {
  expect_message(
    res <- parse_odm_to_long_df("nonexistent.xml"),
    "Error parsing XML file"
  )
  expect_equal(nrow(res), 0)
})

# To test actual parsing, we'd ideally mock an XML file.
test_that("parse_odm_to_long_df parses valid XML", {
  xml_content <- '<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
  <Study OID="S1">
    <MetaDataVersion OID="v1">
      <ItemDef OID="I1" Name="Item1">
        <Question><TranslatedText>Question 1</TranslatedText></Question>
      </ItemDef>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="S1">
    <SubjectData SubjectKey="SUBJ1">
      <StudyEventData StudyEventOID="SE1">
        <FormData FormOID="F1">
          <ItemGroupData ItemGroupOID="IG1">
            <ItemData ItemOID="I1" Value="Val1"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>'

  tmp_file <- tempfile(fileext = ".xml")
  writeLines(xml_content, tmp_file)

  res <- parse_odm_to_long_df(tmp_file)
  expect_equal(nrow(res), 1)
  expect_equal(res$SubjectKey, "SUBJ1")
  expect_equal(res$ItemOID, "I1")
  expect_equal(res$Value, "Val1")
  expect_equal(res$Question, "Question 1")
  expect_equal(res$ItemName, "Item1")

  unlink(tmp_file)
})
