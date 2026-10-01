# Reference solution for the yamaa benchmark sdtm-cm-odm-repeated (R track).
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

# One course per form repeat within a visit repeat: subject, visit, visit
# repeat, and form repeat. A course without a reported treatment name gives
# no record; a repeat number reused at a later visit starts a new course.
courses <- odm |>
  filter(ItemGroupOID == "IG.CM") |>
  pivot_wider(
    id_cols = c(
      StudyOID, SubjectKey, StudyEventOID, StudyEventRepeatKey,
      ItemGroupRepeatKey
    ),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    CMTRT = `IT.CM.CMTRT`,
    CMSTDTC = `IT.CM.CMSTDTC`,
    CMENDTC = `IT.CM.CMENDTC`,
    CMROUTE = `IT.CM.CMROUTE`,
    CMINDC = `IT.CM.CMINDC`
  ) |>
  filter(!is.na(CMTRT) & CMTRT != "") |>
  # Courses run by visit (screening before baseline), visit repeat, and
  # form repeat; the same medication in two courses stays two records.
  mutate(
    VISITORD = case_when(
      StudyEventOID == "SCREENING" ~ 0L,
      StudyEventOID == "BASELINE" ~ 1L,
      .default = 2L
    ),
    VISITREP = as.integer(StudyEventRepeatKey),
    IGREP = as.integer(ItemGroupRepeatKey)
  ) |>
  arrange(SubjectKey, VISITORD, VISITREP, IGREP) |>
  group_by(SubjectKey) |>
  mutate(CMSEQ = row_number()) |>
  ungroup() |>
  mutate(DOMAIN = "CM", STUDYID = StudyOID, USUBJID = SubjectKey) |>
  select(
    DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT,
    CMSTDTC, CMENDTC, CMROUTE, CMINDC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(courses, "/app/output/cm.csv", na = "")
