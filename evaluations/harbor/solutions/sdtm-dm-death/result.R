# Reference solution for the yamaa benchmark sdtm-dm-death (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm_raw <- read_csv(
  "/app/input/dm_raw.csv",
  col_types = cols(.default = col_character())
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(.default = col_character())
)
ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(.default = col_character())
)

ds_death <- ds |>
  filter(DSDECOD == "DEATH") |>
  mutate(DSSTDTC = suppressWarnings(as.Date(DSSTDTC))) |>
  filter(!is.na(DSSTDTC)) |>
  group_by(STUDYID, USUBJID) |>
  summarise(DS_DTH = max(DSSTDTC), .groups = "drop")

ae_fatal <- ae |>
  filter(AEOUT == "FATAL") |>
  mutate(AEENDTC = suppressWarnings(as.Date(AEENDTC))) |>
  filter(!is.na(AEENDTC)) |>
  group_by(STUDYID, USUBJID) |>
  summarise(AE_DTH = max(AEENDTC), .groups = "drop")

dm <- dm_raw |>
  left_join(ds_death, by = c("STUDYID", "USUBJID")) |>
  left_join(ae_fatal, by = c("STUDYID", "USUBJID")) |>
  mutate(
    DOMAIN = "DM",
    DTHDTC = coalesce(DS_DTH, AE_DTH),
    DTHFL = ifelse(!is.na(DTHDTC), "Y", NA_character_)
  ) |>
  select(DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
