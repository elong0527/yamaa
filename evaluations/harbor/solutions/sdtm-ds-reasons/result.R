# Reference solution for the yamaa benchmark sdtm-ds-reasons (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

wide <- odm |>
  filter(StudyEventOID %in% c("EOT", "EOS")) |>
  group_by(StudyOID, SubjectKey, StudyEventOID) |>
  summarise(
    DTC = Value[ItemOID == "IT.DS.DTC"][1],
    COMP = Value[ItemOID == "IT.DS.COMP"][1],
    REASON = Value[ItemOID == "IT.DS.REASON"][1],
    REASONCD = Value[ItemOID == "IT.DS.REASONCD"][1],
    .groups = "drop"
  )

ds <- wide |>
  mutate(
    DOMAIN = "DS",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    DSSEQ = ifelse(StudyEventOID == "EOT", 1L, 2L),
    DSCAT = "DISPOSITION EVENT",
    DSSCAT = ifelse(StudyEventOID == "EOT", "STUDY TREATMENT", "STUDY"),
    DSTERM = ifelse(COMP == "COMPLETED", "COMPLETED", na_if(REASON, "")),
    DSDECOD = ifelse(COMP == "COMPLETED", "COMPLETED", na_if(REASONCD, "")),
    DSSTDTC = suppressWarnings(as.Date(na_if(DTC, "")))
  ) |>
  select(DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC) |>
  arrange(USUBJID, DSSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ds, "/app/output/ds.csv", na = "")
