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

test_that("parse_odm_to_long_df handles prefixed namespace equivalently", {
  default_xml <- '<?xml version="1.0" encoding="UTF-8"?>
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
      <StudyEventData StudyEventOID="SE1" StudyEventRepeatKey="1">
        <FormData FormOID="F1">
          <ItemGroupData ItemGroupOID="IG1" ItemGroupRepeatKey="1">
            <ItemData ItemOID="I1" Value="Val1"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>'

  prefixed_xml <- '<?xml version="1.0" encoding="UTF-8"?>
<odm:ODM xmlns:odm="http://www.cdisc.org/ns/odm/v1.3">
  <odm:Study OID="S1">
    <odm:MetaDataVersion OID="v1">
      <odm:ItemDef OID="I1" Name="Item1">
        <odm:Question><odm:TranslatedText>Question 1</odm:TranslatedText></odm:Question>
      </odm:ItemDef>
    </odm:MetaDataVersion>
  </odm:Study>
  <odm:ClinicalData StudyOID="S1">
    <odm:SubjectData SubjectKey="SUBJ1">
      <odm:StudyEventData StudyEventOID="SE1" StudyEventRepeatKey="1">
        <odm:FormData FormOID="F1">
          <odm:ItemGroupData ItemGroupOID="IG1" ItemGroupRepeatKey="1">
            <odm:ItemData ItemOID="I1" Value="Val1"/>
          </odm:ItemGroupData>
        </odm:FormData>
      </odm:StudyEventData>
    </odm:SubjectData>
  </odm:ClinicalData>
</odm:ODM>'

  tmp_default <- tempfile(fileext = ".xml")
  tmp_prefixed <- tempfile(fileext = ".xml")
  writeLines(default_xml, tmp_default)
  writeLines(prefixed_xml, tmp_prefixed)

  res_default <- parse_odm_to_long_df(tmp_default)
  res_prefixed <- parse_odm_to_long_df(tmp_prefixed)

  expect_equal(nrow(res_default), 1)
  expect_equal(nrow(res_prefixed), 1)
  expect_equal(res_prefixed$SubjectKey, res_default$SubjectKey)
  expect_equal(res_prefixed$ItemOID, res_default$ItemOID)
  expect_equal(res_prefixed$Value, res_default$Value)
  expect_equal(res_prefixed$Question, res_default$Question)
  expect_equal(res_prefixed$ItemName, res_default$ItemName)

  unlink(tmp_default)
  unlink(tmp_prefixed)
})

test_that("parse_odm_to_long_df preserves multiple repeat levels for both namespace forms", {
  build_xml <- function(use_prefix) {
    ns_decl <- if (use_prefix) 'xmlns:odm="http://www.cdisc.org/ns/odm/v1.3"' else 'xmlns="http://www.cdisc.org/ns/odm/v1.3"'
    root_open <- if (use_prefix) "<odm:ODM" else "<ODM"
    root_close <- if (use_prefix) "</odm:ODM>" else "</ODM>"
    p <- if (use_prefix) "odm:" else ""
    sprintf(
      '<?xml version="1.0" encoding="UTF-8"?>\n%s %s>\n  <%sStudy OID="S1">\n    <%sMetaDataVersion OID="v1">\n      <%sItemDef OID="I1" Name="Item1"><%sQuestion><%sTranslatedText>Q1</%sTranslatedText></%sQuestion></%sItemDef>\n    </%sMetaDataVersion>\n  </%sStudy>\n  <%sClinicalData StudyOID="S1">\n    <%sSubjectData SubjectKey="SUBJ1">\n      <%sStudyEventData StudyEventOID="SE1" StudyEventRepeatKey="1">\n        <%sFormData FormOID="F1">\n          <%sItemGroupData ItemGroupOID="IG1" ItemGroupRepeatKey="1"><%sItemData ItemOID="I1" Value="100"/></%sItemGroupData>\n          <%sItemGroupData ItemGroupOID="IG1" ItemGroupRepeatKey="2"><%sItemData ItemOID="I1" Value="200"/></%sItemGroupData>\n        </%sFormData>\n      </%sStudyEventData>\n      <%sStudyEventData StudyEventOID="SE1" StudyEventRepeatKey="2">\n        <%sFormData FormOID="F1">\n          <%sItemGroupData ItemGroupOID="IG1" ItemGroupRepeatKey="1"><%sItemData ItemOID="I1" Value="300"/></%sItemGroupData>\n        </%sFormData>\n      </%sStudyEventData>\n    </%sSubjectData>\n  </%sClinicalData>\n%s',
      root_open, ns_decl,
      p, p, p, p, p, p, p, p,
      p, p,
      p, p, p, p, p, p, p, p, p, p,
      p, p, p, p, p, p,
      root_close
    )
  }

  tmp_default <- tempfile(fileext = ".xml")
  tmp_prefixed <- tempfile(fileext = ".xml")
  writeLines(build_xml(FALSE), tmp_default)
  writeLines(build_xml(TRUE), tmp_prefixed)

  res_default <- parse_odm_to_long_df(tmp_default)
  res_prefixed <- parse_odm_to_long_df(tmp_prefixed)

  expect_equal(nrow(res_default), 3)
  expect_equal(nrow(res_prefixed), 3)
  expect_equal(sort(res_default$Value), c("100", "200", "300"))
  expect_equal(sort(res_prefixed$Value), c("100", "200", "300"))
  expect_equal(sort(unique(res_default$StudyEventRepeatKey)), c("1", "2"))
  expect_equal(sort(unique(res_default$ItemGroupRepeatKey)), c("1", "2"))
  expect_equal(nrow(res_default), nrow(res_prefixed))

  unlink(tmp_default)
  unlink(tmp_prefixed)
})

test_that("parse_odm_to_long_df rejects unsupported namespace", {
  wrong_ns_xml <- '<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://example.com/other">
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

  no_ns_xml <- '<?xml version="1.0" encoding="UTF-8"?>
<ODM>
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

  tmp_wrong <- tempfile(fileext = ".xml")
  tmp_none <- tempfile(fileext = ".xml")
  writeLines(wrong_ns_xml, tmp_wrong)
  writeLines(no_ns_xml, tmp_none)

  expect_error(parse_odm_to_long_df(tmp_wrong), "Unsupported ODM namespace")
  expect_error(parse_odm_to_long_df(tmp_none), "Unsupported ODM namespace")

  unlink(tmp_wrong)
  unlink(tmp_none)
})
