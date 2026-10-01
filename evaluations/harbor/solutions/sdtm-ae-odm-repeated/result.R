# Reference solution for the yamaa benchmark sdtm-ae-odm-repeated (R track).
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

# One event per AE item group occurrence: subject, visit, visit repeat, item
# group, and item group repeat key. An occurrence without a reported term
# gives no record; a repeat key reused at a later visit starts a new event.
events <- odm |>
  filter(ItemGroupOID == "IG.AE") |>
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
    AETERM = `IT.AE.AETERM`,
    AESTDTC = `IT.AE.AESTDTC`,
    AEENDTC = `IT.AE.AEENDTC`,
    AESEV = `IT.AE.AESEV`,
    AESER = `IT.AE.AESER`
  ) |>
  filter(!is.na(AETERM) & AETERM != "") |>
  # Visit order is screening, then baseline; then visit repeat and item
  # group repeat key. The same term in two occurrences stays two records.
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
  mutate(AESEQ = row_number()) |>
  ungroup() |>
  mutate(DOMAIN = "AE", STUDYID = StudyOID, USUBJID = SubjectKey) |>
  select(
    DOMAIN, STUDYID, USUBJID, AESEQ, AETERM,
    AESTDTC, AEENDTC, AESEV, AESER
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(events, "/app/output/ae.csv", na = "")
