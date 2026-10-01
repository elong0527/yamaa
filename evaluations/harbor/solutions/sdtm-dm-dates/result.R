# Reference solution for the yamaa benchmark sdtm-dm-dates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm_raw <- read_csv(
  "/app/input/dm_raw.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
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

ex_sum <- ex |>
  mutate(
    EXSTDTC = suppressWarnings(as.Date(EXSTDTC)),
    EXENDTC = suppressWarnings(as.Date(EXENDTC))
  ) |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    RFXSTDTC = suppressWarnings(min(EXSTDTC, na.rm = TRUE)),
    RFXENDTC = suppressWarnings(max(EXENDTC, na.rm = TRUE)),
    .groups = "drop"
  ) |>
  mutate(
    RFXSTDTC = as.Date(ifelse(is.infinite(as.numeric(RFXSTDTC)), NA, RFXSTDTC), origin = "1970-01-01"),
    RFXENDTC = as.Date(ifelse(is.infinite(as.numeric(RFXENDTC)), NA, RFXENDTC), origin = "1970-01-01")
  )

ds_sum <- ds |>
  filter(DSCAT == "DISPOSITION EVENT") |>
  mutate(DSSTDTC = suppressWarnings(as.Date(DSSTDTC))) |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    DS_MAX = suppressWarnings(max(DSSTDTC, na.rm = TRUE)),
    .groups = "drop"
  ) |>
  mutate(
    DS_MAX = as.Date(ifelse(is.infinite(as.numeric(DS_MAX)), NA, DS_MAX), origin = "1970-01-01")
  )

ae_sum <- ae |>
  mutate(AEENDTC = suppressWarnings(as.Date(AEENDTC))) |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    AE_MAX = suppressWarnings(max(AEENDTC, na.rm = TRUE)),
    .groups = "drop"
  ) |>
  mutate(
    AE_MAX = as.Date(ifelse(is.infinite(as.numeric(AE_MAX)), NA, AE_MAX), origin = "1970-01-01")
  )

dm <- dm_raw |>
  left_join(ex_sum, by = c("STUDYID", "USUBJID")) |>
  left_join(ds_sum, by = c("STUDYID", "USUBJID")) |>
  left_join(ae_sum, by = c("STUDYID", "USUBJID")) |>
  mutate(
    DOMAIN = "DM",
    RFICDTC = suppressWarnings(as.Date(RFICDTC)),
    RFSTDTC = RFXSTDTC,
    RFPENDTC = pmax(RFXENDTC, DS_MAX, AE_MAX, na.rm = TRUE),
    RFPENDTC = as.Date(ifelse(is.infinite(as.numeric(RFPENDTC)), NA, RFPENDTC), origin = "1970-01-01"),
    RFENDTC = ifelse(!is.na(RFSTDTC), as.character(RFPENDTC), NA),
    RFENDTC = suppressWarnings(as.Date(RFENDTC, origin = "1970-01-01"))
  ) |>
  select(DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC, RFENDTC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
