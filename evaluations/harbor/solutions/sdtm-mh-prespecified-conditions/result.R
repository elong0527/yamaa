# Reference solution for the yamaa benchmark sdtm-mh-prespecified-conditions (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

items <- read_csv(
  "/app/input/mh_items.csv",
  col_types = cols(SORTORD = col_integer(), .default = col_character())
)
odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

checklist_oids <- intersect(items$ItemOID, unique(odm$ItemOID))

checklist <- odm |>
  filter(ItemOID %in% checklist_oids) |>
  left_join(items |> select(ItemOID, MHTERM, SORTORD), by = "ItemOID") |>
  mutate(
    answered = Value %in% c("Y", "N"),
    MHTERM = MHTERM,
    MHCAT = "DISEASE-SPECIFIC HISTORY",
    MHPRESP = "Y",
    MHOCCUR = if_else(answered, Value, NA_character_),
    MHSTAT = if_else(answered, NA_character_, "NOT DONE"),
    visit_rank = 0L,
    repeat_no = 0L
  ) |>
  select(
    STUDYID = StudyOID, USUBJID = SubjectKey, ItemOID, MHTERM, MHCAT,
    MHPRESP, MHOCCUR, MHSTAT, SORTORD, visit_rank, repeat_no
  )

volunteered <- odm |>
  filter(!ItemOID %in% checklist_oids, !is.na(Value) & Value != "") |>
  mutate(
    MHTERM = Value,
    MHCAT = "GENERAL HISTORY",
    MHPRESP = NA_character_,
    MHOCCUR = NA_character_,
    MHSTAT = NA_character_,
    SORTORD = NA_integer_,
    visit_rank = case_when(
      StudyEventOID == "SCREENING" ~ 0L,
      StudyEventOID == "BASELINE" ~ 1L,
      TRUE ~ 99L
    ),
    repeat_no = suppressWarnings(as.integer(ItemGroupRepeatKey))
  ) |>
  select(
    STUDYID = StudyOID, USUBJID = SubjectKey, ItemOID, MHTERM, MHCAT,
    MHPRESP, MHOCCUR, MHSTAT, SORTORD, visit_rank, repeat_no
  )

mh <- bind_rows(checklist, volunteered) |>
  arrange(USUBJID, is.na(SORTORD), SORTORD, visit_rank, repeat_no) |>
  group_by(USUBJID) |>
  mutate(MHSEQ = row_number()) |>
  ungroup() |>
  mutate(DOMAIN = "MH") |>
  arrange(USUBJID, MHSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(mh, "/app/output/mh.csv", na = "")
