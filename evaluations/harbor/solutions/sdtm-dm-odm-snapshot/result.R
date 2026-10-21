# Reference solution for the yamaa benchmark sdtm-dm-odm-snapshot (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(xml2)

odm <- read_xml("/app/input/odm.xml")
# The snapshot's elements are in the ODM 1.3 default namespace; strip it so
# the paths below name the elements plainly.
xml_ns_strip(odm)

# One subject's current value of one item. An item recorded as null has no
# value; audit records, signatures and comments hold no items.
item_value <- function(subject, oid) {
  item <- xml_find_first(subject, sprintf(".//ItemData[@ItemOID='%s']", oid))
  if (inherits(item, "xml_missing")) {
    return(NA_character_)
  }
  is_null <- xml_attr(item, "IsNull")
  if (!is.na(is_null) && tolower(is_null) %in% c("yes", "true", "1")) {
    return(NA_character_)
  }
  xml_attr(item, "Value")
}

rows <- lapply(
  xml_find_all(odm, "/ODM/ClinicalData/SubjectData"),
  function(subject) {
    study <- xml_attr(xml_parent(subject), "StudyOID")
    key <- xml_attr(subject, "SubjectKey")
    age <- item_value(subject, "IT.DM.AGE")
    data.frame(
      DOMAIN = "DM",
      STUDYID = study,
      USUBJID = paste0(study, "-", key),
      SUBJID = key,
      BRTHDTC = item_value(subject, "IT.DM.BRTHDAT"),
      AGE = as.integer(age),
      AGEU = if (is.na(age)) NA_character_ else "YEARS",
      SEX = item_value(subject, "IT.DM.SEX"),
      stringsAsFactors = FALSE
    )
  }
)
dm <- do.call(rbind, rows)

write.csv(dm, "/app/output/dm.csv", row.names = FALSE, na = "")
