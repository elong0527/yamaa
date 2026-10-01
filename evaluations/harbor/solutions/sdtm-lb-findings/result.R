# Reference solution for the yamaa benchmark sdtm-lb-findings (R track).
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

lb <- odm |>
  filter(ItemOID %in% c("IT.LB.CALCIUM", "IT.LB.CREAT")) |>
  left_join(dates, by = key_cols) |>
  mutate(Value = na_if(Value, "")) |>
  filter(!is.na(Value)) |>
  mutate(
    LBTESTCD = if_else(ItemOID == "IT.LB.CALCIUM", "CA", "CREAT"),
    LBTEST = if_else(ItemOID == "IT.LB.CALCIUM", "Calcium", "Creatinine"),
    NOTDONE = Value == "NOT DONE",
    DOMAIN = "LB",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    LBORRES = if_else(NOTDONE, NA_character_, Value),
    LBORRESU = if_else(NOTDONE, NA_character_, "mg/dL"),
    LBSTRESN = if_else(
      NOTDONE, NA_real_, suppressWarnings(as.numeric(Value))
    ),
    LBSTRESU = if_else(NOTDONE, NA_character_, "mg/dL"),
    LBSTAT = if_else(NOTDONE, "NOT DONE", NA_character_)
  ) |>
  arrange(STUDYID, USUBJID, LBDTC, LBTESTCD) |>
  group_by(STUDYID, USUBJID) |>
  mutate(LBSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES,
    LBORRESU, LBSTRESN, LBSTRESU, LBSTAT, LBDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
