# Reference solution for the yamaa benchmark sdtm-cm-whodrug-coding (R track).
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
coding <- read_csv(
  "/app/input/coding_output.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)
whodrug <- read_csv(
  "/app/input/whodrug_extract.csv",
  col_types = cols(.default = col_character())
)

# One record per reported medication; the repeat number is the sequence
# number. The preferred name comes from the drug record the coder chose,
# and the class from the ATC code assigned for this use, so the same drug
# record can give a different class on different records. No coding, or a
# code not in the extract, leaves the coded variables empty.
reported <- odm |>
  filter(ItemGroupOID == "IG.CM") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, ItemGroupRepeatKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    CMSEQ = as.integer(ItemGroupRepeatKey),
    CMTRT = `IT.CM.CMTRT`
  ) |>
  select(STUDYID, USUBJID, CMSEQ, CMTRT)

cm <- reported |>
  left_join(coding, by = c("STUDYID", "USUBJID", "CMSEQ")) |>
  left_join(whodrug, by = c("DRUG_RECORD_NO", "ATC_CODE")) |>
  mutate(
    DOMAIN = "CM",
    CMDECOD = if_else(
      is.na(DRUG_RECORD_NO) | DRUG_RECORD_NO == "" | is.na(PREFERRED_NAME),
      NA_character_, PREFERRED_NAME
    ),
    CMCLAS = if_else(is.na(CMDECOD), NA_character_, ATC_CLASS_NAME),
    CMCLASCD = if_else(is.na(CMDECOD), NA_character_, ATC_CLASS_CODE)
  ) |>
  arrange(USUBJID, CMSEQ) |>
  select(DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, CMCLAS, CMCLASCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(cm, "/app/output/cm.csv", na = "")
