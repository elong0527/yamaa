# Reference solution for the yamaa benchmark sdtm-qs-questionnaire-items (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

items <- read_csv(
  "/app/input/items.csv",
  col_types = cols(ItemOrder = col_integer(), .default = col_character())
)
visits <- read_csv(
  "/app/input/visits.csv",
  col_types = cols(VISITNUM = col_integer(), .default = col_character())
)
notdone <- read_csv(
  "/app/input/notdone.csv",
  col_types = cols(.default = col_character())
)
odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

labels <- c(
  "0" = "Not at all",
  "1" = "Several days",
  "2" = "More than half the days",
  "3" = "Nearly every day"
)

scores <- odm |>
  filter(!is.na(Value) & Value != "") |>
  select(SubjectKey, StudyEventOID, ItemOID, Value)

skipped <- notdone |>
  filter(!is.na(ItemOID) & ItemOID != "") |>
  select(SubjectKey, StudyEventOID, ItemOID, Reason)

refused <- notdone |>
  filter(is.na(ItemOID) | ItemOID == "") |>
  select(SubjectKey, StudyEventOID, Reason)

first_visit <- visits |>
  group_by(SubjectKey) |>
  summarise(first_num = min(VISITNUM), .groups = "drop")

# Every item at each visit, with the collected score or a NOT DONE record.
item_rows <- visits |>
  left_join(first_visit, by = "SubjectKey") |>
  cross_join(items) |>
  left_join(
    scores,
    by = c("SubjectKey", "StudyEventOID", "ItemOID")
  ) |>
  left_join(
    skipped,
    by = c("SubjectKey", "StudyEventOID", "ItemOID")
  ) |>
  left_join(
    refused |> select(SubjectKey, StudyEventOID) |> mutate(is_refused = TRUE),
    by = c("SubjectKey", "StudyEventOID")
  ) |>
  # A refused questionnaire contributes only its QSALL record below.
  filter(is.na(is_refused)) |>
  mutate(
    DOMAIN = "QS",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    QSTESTCD = QSTESTCD,
    QSTEST = QSTEST,
    QSCAT = "PHQ-9",
    answered = !is.na(Value) & Value != "",
    QSORRES = if_else(answered, unname(labels[Value]), NA_character_),
    QSSTRESC = if_else(answered, Value, NA_character_),
    QSSTRESN = suppressWarnings(as.numeric(QSSTRESC)),
    QSSTAT = if_else(answered, NA_character_, "NOT DONE"),
    QSREASND = if_else(answered, NA_character_, Reason),
    QSBLFL = if_else(VISITNUM == first_num, "Y", NA_character_),
    QSDRVFL = NA_character_,
    QSDTC = VisitDate
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
    QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC, Value
  )

totals <- item_rows |>
  group_by(STUDYID, USUBJID, VISITNUM, QSDTC, QSBLFL) |>
  summarise(
    n_answered = sum(!is.na(Value)),
    total = sum(suppressWarnings(as.numeric(Value)), na.rm = TRUE),
    .groups = "drop"
  ) |>
  filter(n_answered == 9) |>
  mutate(
    DOMAIN = "QS",
    QSTESTCD = "PHQ9T",
    QSTEST = "Patient Health Questionnaire 9 item total score",
    QSCAT = "PHQ-9",
    QSORRES = NA_character_,
    QSSTRESC = as.character(total),
    QSSTRESN = total,
    QSSTAT = NA_character_,
    QSREASND = NA_character_,
    QSDRVFL = "Y"
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
    QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC
  )

refused_rows <- visits |>
  inner_join(refused, by = c("SubjectKey", "StudyEventOID")) |>
  left_join(first_visit, by = "SubjectKey") |>
  mutate(
    DOMAIN = "QS",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    QSTESTCD = "QSALL",
    QSTEST = "All Questionnaires",
    QSCAT = "PHQ-9",
    QSORRES = NA_character_,
    QSSTRESC = NA_character_,
    QSSTRESN = NA_real_,
    QSSTAT = "NOT DONE",
    QSREASND = Reason,
    QSBLFL = if_else(VISITNUM == first_num, "Y", NA_character_),
    QSDRVFL = NA_character_,
    QSDTC = VisitDate
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
    QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC
  )

qs <- bind_rows(
  item_rows |>
    select(-Value) |>
    mutate(QSSTRESN = suppressWarnings(as.numeric(QSSTRESC))),
  totals,
  refused_rows
) |>
  arrange(USUBJID, VISITNUM, QSTESTCD) |>
  group_by(USUBJID) |>
  mutate(QSSEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, QSSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES,
    QSSTRESC, QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(qs, "/app/output/qs.csv", na = "")
