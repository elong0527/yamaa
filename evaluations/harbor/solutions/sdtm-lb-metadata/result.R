# Reference solution for the yamaa benchmark sdtm-lb-metadata (R track).
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

key_cols <- c(
  "StudyOID", "SubjectKey", "StudyEventOID", "StudyEventRepeatKey",
  "ItemGroupOID", "ItemGroupRepeatKey"
)

dates <- odm |>
  filter(ItemOID == "IT.LB.LBDTC") |>
  select(all_of(key_cols), LBDTC = Value)

tests <- odm |>
  filter(ItemOID %in% c("IT.LB.GLUC", "IT.LB.CREAT")) |>
  left_join(dates, by = key_cols) |>
  mutate(
    LBTESTCD = if_else(ItemOID == "IT.LB.GLUC", "GLUC", "CREAT"),
    LBTEST = if_else(ItemOID == "IT.LB.GLUC", "Glucose", "Creatinine"),
    LBORRES = na_if(Value, ""),
  ) |>
  filter(!is.na(LBORRES)) |>
  mutate(
    DOMAIN = "LB",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    LBORRESU = "mg/dL",
    LBSTRESN = suppressWarnings(as.numeric(LBORRES)),
    LBSTRESU = "mg/dL",
  ) |>
  arrange(STUDYID, USUBJID, LBDTC, LBTESTCD) |>
  group_by(STUDYID, USUBJID) |>
  mutate(LBSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES,
    LBORRESU, LBSTRESN, LBSTRESU, LBDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(tests, "/app/output/lb.csv", na = "")
