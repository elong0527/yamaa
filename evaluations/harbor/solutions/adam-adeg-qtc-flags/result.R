# Reference solution for the yamaa benchmark adam-adeg-qtc-flags (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adeg_raw.csv",
  col_types = cols(
    AVAL = col_double(),
    BASE = col_double(),
    CHG = col_double(),
    .default = col_character()
  )
)

# Only on-treatment QTcF records are assessed, and values are compared
# unrounded, strictly above each limit: exactly at a limit reads N.
absolute <- function(data) {
  data$PARAMCD %in% "QTCF" & data$ONTRTFL %in% "Y" & !is.na(data$AVAL)
}
change <- function(data) {
  data$PARAMCD %in% "QTCF" & data$ONTRTFL %in% "Y" & !is.na(data$CHG)
}

adeg <- raw |>
  mutate(
    CRIT1 = if_else(absolute(raw), "QTcF > 450 ms", NA_character_),
    CRIT1FL = case_when(
      !absolute(raw) ~ NA_character_,
      AVAL > 450 ~ "Y",
      .default = "N"
    ),
    CRIT2 = if_else(absolute(raw), "QTcF > 480 ms", NA_character_),
    CRIT2FL = case_when(
      !absolute(raw) ~ NA_character_,
      AVAL > 480 ~ "Y",
      .default = "N"
    ),
    CRIT3 = if_else(absolute(raw), "QTcF > 500 ms", NA_character_),
    CRIT3FL = case_when(
      !absolute(raw) ~ NA_character_,
      AVAL > 500 ~ "Y",
      .default = "N"
    ),
    CRIT4 = if_else(
      change(raw), "QTcF increase from baseline > 30 ms", NA_character_
    ),
    CRIT4FL = case_when(
      !change(raw) ~ NA_character_,
      CHG > 30 ~ "Y",
      .default = "N"
    ),
    CRIT5 = if_else(
      change(raw), "QTcF increase from baseline > 60 ms", NA_character_
    ),
    CRIT5FL = case_when(
      !change(raw) ~ NA_character_,
      CHG > 60 ~ "Y",
      .default = "N"
    )
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, ABLFL, ONTRTFL,
    BASE, CHG, CRIT1, CRIT1FL, CRIT2, CRIT2FL, CRIT3, CRIT3FL, CRIT4,
    CRIT4FL, CRIT5, CRIT5FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg, "/app/output/adeg.csv", na = "")
