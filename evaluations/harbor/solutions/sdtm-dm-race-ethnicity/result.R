# Reference solution for the yamaa benchmark sdtm-dm-race-ethnicity (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(stringr)
library(purrr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

race_map <- c(
  "White" = "WHITE",
  "Asian" = "ASIAN",
  "Black or African American" = "BLACK OR AFRICAN AMERICAN",
  "American Indian or Alaska Native" = "AMERICAN INDIAN OR ALASKA NATIVE",
  "Native Hawaiian or Other Pacific Islander" = "NATIVE HAWAIIAN OR OTHER PACIFIC ISLANDER",
  "Other, specify: Fijian" = "OTHER",
  "Subject refused" = "UNKNOWN",
  "Not reported" = "NOT REPORTED"
)

ethnic_map <- c(
  "Hispanic or Latino" = "HISPANIC OR LATINO",
  "Not Hispanic or Latino" = "NOT HISPANIC OR LATINO",
  "Not reported" = "NOT REPORTED",
  "Unknown" = "UNKNOWN"
)

map_term <- function(x, lut) {
  out <- unname(lut[x])
  out[is.na(out)] <- NA_character_
  out
}

subjects <- odm |>
  distinct(StudyOID, SubjectKey) |>
  mutate(
    STUDYID = StudyOID,
    USUBJID = ifelse(
      str_starts(SubjectKey, fixed(StudyOID)),
      SubjectKey,
      paste(StudyOID, SubjectKey, sep = "-")
    )
  )

race_distinct <- odm |>
  filter(ItemOID == "IT.DM.RACE", Value != "", !is.na(Value)) |>
  distinct(StudyOID, SubjectKey, Value)

race_per_subject <- race_distinct |>
  group_by(StudyOID, SubjectKey) |>
  summarise(answers = list(sort(unique(Value))), .groups = "drop")

ethnic_per_subject <- odm |>
  filter(ItemOID == "IT.DM.ETHNIC", Value != "", !is.na(Value)) |>
  group_by(StudyOID, SubjectKey) |>
  summarise(ethnic_answer = first(Value), .groups = "drop")

dm <- subjects |>
  left_join(race_per_subject, by = c("StudyOID", "SubjectKey")) |>
  left_join(ethnic_per_subject, by = c("StudyOID", "SubjectKey")) |>
  rowwise() |>
  mutate(
    DOMAIN = "DM",
    SUBJID = USUBJID,
    RACE = if (is.null(answers) || length(answers) == 0 || all(is.na(answers))) {
      NA_character_
    } else if (length(answers) >= 2) {
      "MULTIPLE"
    } else {
      map_term(answers[[1]], race_map)
    },
    ETHNIC = map_term(ethnic_answer, ethnic_map)
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, DOMAIN, SUBJID, RACE, ETHNIC, StudyOID, SubjectKey) |>
  arrange(USUBJID)

multi <- dm |> filter(RACE == "MULTIPLE")

suppdm <- race_distinct |>
  inner_join(
    multi |> select(StudyOID, SubjectKey),
    by = c("StudyOID", "SubjectKey")
  ) |>
  left_join(
    subjects |> select(StudyOID, SubjectKey, STUDYID, USUBJID),
    by = c("StudyOID", "SubjectKey")
  ) |>
  arrange(USUBJID, Value) |>
  group_by(STUDYID, USUBJID) |>
  mutate(
    n = row_number(),
    QNAM = paste0("RACE", n),
    QLABEL = paste("Race", n)
  ) |>
  ungroup() |>
  mutate(
    RDOMAIN = "DM",
    IDVAR = "USUBJID",
    IDVARVAL = USUBJID,
    QVAL = map_term(Value, race_map),
    QORIG = "CRF",
    QEVAL = NA_character_
  ) |>
  select(STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL) |>
  arrange(USUBJID, QNAM)

dir.create("/app/output", showWarnings = FALSE)
write_csv(
  dm |> select(DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC),
  "/app/output/dm.csv",
  na = ""
)
write_csv(suppdm, "/app/output/suppdm.csv", na = "")
