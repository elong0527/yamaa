# Reference solution for the yamaa benchmark sdtm-ds-multi-form-disposition (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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
  group_by(StudyOID, SubjectKey, StudyEventOID) |>
  summarise(
    DTC = Value[ItemOID == "IT.DS.DTC"][1],
    COMP = Value[ItemOID == "IT.DS.COMP"][1],
    REASON = Value[ItemOID == "IT.DS.REASON"][1],
    REASONCD = Value[ItemOID == "IT.DS.REASONCD"][1],
    .groups = "drop"
  )

ds <- wide |>
  rowwise() |>
  mutate(
    DSTERM = case_when(
      StudyEventOID == "CONSENT" ~ "INFORMED CONSENT OBTAINED",
      StudyEventOID == "RAND" ~ "RANDOMIZED",
      StudyEventOID == "EOT" & COMP == "COMPLETED" ~ "COMPLETED",
      StudyEventOID == "EOT" ~ REASON,
      StudyEventOID == "EOS" & COMP == "COMPLETED" ~ "COMPLETED",
      StudyEventOID == "EOS" & COMP == "SCREEN FAILURE" ~ "SCREEN FAILURE",
      StudyEventOID == "EOS" ~ REASON,
      .default = NA_character_
    ),
    DSDECOD = case_when(
      StudyEventOID == "CONSENT" ~ "INFORMED CONSENT OBTAINED",
      StudyEventOID == "RAND" ~ "RANDOMIZED",
      StudyEventOID == "EOT" & COMP == "COMPLETED" ~ "COMPLETED",
      StudyEventOID == "EOT" ~ REASONCD,
      StudyEventOID == "EOS" & COMP == "COMPLETED" ~ "COMPLETED",
      StudyEventOID == "EOS" & COMP == "SCREEN FAILURE" ~ "SCREEN FAILURE",
      StudyEventOID == "EOS" ~ REASONCD,
      .default = NA_character_
    ),
    DSCAT = case_when(
      StudyEventOID %in% c("CONSENT", "RAND") ~ "PROTOCOL MILESTONE",
      StudyEventOID %in% c("EOT", "EOS") ~ "DISPOSITION EVENT",
      .default = NA_character_
    ),
    DSSCAT = case_when(
      StudyEventOID == "CONSENT" ~ "INFORMED CONSENT",
      StudyEventOID == "RAND" ~ "RANDOMIZATION",
      StudyEventOID == "EOT" ~ "END OF TREATMENT",
      StudyEventOID == "EOS" ~ "END OF STUDY",
      .default = NA_character_
    )
  ) |>
  ungroup() |>
  filter(StudyEventOID %in% c("CONSENT", "RAND", "EOT", "EOS")) |>
  mutate(
    DOMAIN = "DS",
    STUDYID = StudyOID,
    USUBJID = ifelse(
      startsWith(SubjectKey, StudyOID),
      SubjectKey,
      paste(StudyOID, SubjectKey, sep = "-")
    ),
    DSSTDTC = suppressWarnings(as.Date(na_if(DTC, ""))),
    DSTERM = na_if(DSTERM, ""),
    DSDECOD = na_if(DSDECOD, "")
  ) |>
  arrange(USUBJID, DSSTDTC, DSTERM) |>
  group_by(USUBJID) |>
  mutate(DSSEQ = row_number()) |>
  ungroup() |>
  select(DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ds, "/app/output/ds.csv", na = "")
