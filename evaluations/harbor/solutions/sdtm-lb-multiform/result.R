# Reference solution for the yamaa benchmark sdtm-lb-multiform (R track).
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

test_meta <- tribble(
  ~short, ~LBTESTCD, ~LBTEST, ~LBCAT, ~LBSPEC, ~LBLOC, ~unit,
  "VITD", "VITD25OH", "25-Hydroxyvitamin D", "CHEMISTRY", "SERUM", NA_character_, "ng/mL",
  "IL13_LES", "IL13", "Interleukin 13 mRNA", "GENE EXPRESSION", "SKIN BIOPSY", "LESIONAL", "CYCLE",
  "IL13_NONLES", "IL13", "Interleukin 13 mRNA", "GENE EXPRESSION", "SKIN BIOPSY", "NON-LESIONAL", "CYCLE",
  "SAL_CAMP", "CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "SALIVA", NA_character_, "ng/mL",
  "TS_CAMP_LES", "CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "TAPE STRIP", "LESIONAL", "ng/mL",
  "TS_CAMP_NONLES", "CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "TAPE STRIP", "NON-LESIONAL", "ng/mL"
)

wide <- odm |>
  mutate(
    short = sub(".*\\.", "", ItemOID),
    is_date = grepl("DTC$", short)
  )

dates <- wide |>
  filter(is_date, !is.na(Value) & Value != "") |>
  select(
    StudyOID, SubjectKey, StudyEventOID, ItemGroupOID, ItemGroupRepeatKey,
    LBDTC = Value
  )

results <- wide |>
  filter(!is_date, short %in% test_meta$short, !is.na(Value) & Value != "") |>
  left_join(dates,
    by = c(
      "StudyOID", "SubjectKey", "StudyEventOID", "ItemGroupOID",
      "ItemGroupRepeatKey"
    )
  ) |>
  left_join(test_meta, by = "short") |>
  mutate(
    event_short = sub(".*\\.", "", StudyEventOID),
    VISIT = case_when(
      event_short == "SCRN" ~ "SCREENING",
      event_short == "BL" ~ "BASELINE",
      event_short == "D21" ~ "DAY 21",
      event_short == "UNSCH" ~ "UNSCHEDULED"
    ),
    VISITNUM = case_when(
      event_short == "SCRN" ~ "1",
      event_short == "BL" ~ "2",
      event_short == "D21" ~ "3",
      event_short == "UNSCH" & ItemGroupRepeatKey == "1" ~ "2.01",
      event_short == "UNSCH" & ItemGroupRepeatKey == "2" ~ "2.02",
      TRUE ~ paste0("2.0", ItemGroupRepeatKey)
    ),
    not_done = Value == "NOT DONE",
    LBORRES = if_else(not_done, NA_character_, Value),
    LBORRESU = if_else(not_done, NA_character_, unit),
    LBSTRESC = if_else(not_done, NA_character_, Value),
    LBSTRESN = suppressWarnings(as.numeric(if_else(not_done, NA_character_, Value))),
    LBSTRESU = if_else(not_done, NA_character_, unit),
    LBSTAT = if_else(not_done, "NOT DONE", NA_character_),
    loc_rank = case_when(
      LBLOC == "LESIONAL" ~ 0L,
      LBLOC == "NON-LESIONAL" ~ 1L,
      TRUE ~ 2L
    )
  ) |>
  arrange(SubjectKey, LBDTC, LBTESTCD, LBSPEC, loc_rank) |>
  group_by(SubjectKey) |>
  mutate(LBSEQ = row_number()) |>
  ungroup() |>
  mutate(DOMAIN = "LB") |>
  arrange(SubjectKey, LBSEQ) |>
  select(
    DOMAIN, STUDYID = StudyOID, USUBJID = SubjectKey, LBSEQ, VISIT, VISITNUM,
    LBTESTCD, LBTEST, LBCAT, LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC,
    LBSTRESN, LBSTRESU, LBSTAT, LBDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(results, "/app/output/lb.csv", na = "")
