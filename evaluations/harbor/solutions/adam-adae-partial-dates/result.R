# Reference solution for the yamaa benchmark adam-adae-partial-dates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)
library(stringr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
) |>
  select(USUBJID, TRTSDT)

adae <- ae |>
  left_join(adsl, by = "USUBJID") |>
  mutate(
    is_full = str_detect(AESTDTC, "^\\d{4}-\\d{2}-\\d{2}$"),
    is_month = str_detect(AESTDTC, "^\\d{4}-\\d{2}$"),
    # A year and month is completed to the 15th.
    completed = if_else(
      is_month,
      ymd(paste0(AESTDTC, "-15"), quiet = TRUE),
      ymd(AESTDTC, quiet = TRUE)
    ),
    completed = if_else(is_full | is_month, completed, as.Date(NA)),
    # A completed date is never placed before first exposure. When the
    # whole collected month ends before the exposure date, no day it
    # allows can satisfy that, so the event is left without a date.
    month_end = if_else(
      is_month,
      completed + days(days_in_month(completed) - 15L),
      completed
    ),
    month_before_trt = is_month &
      !is.na(month_end) & !is.na(TRTSDT) & month_end < TRTSDT,
    ASTDT = case_when(
      month_before_trt ~ as.Date(NA),
      is_month & !is.na(completed) & !is.na(TRTSDT) & completed < TRTSDT ~ TRTSDT,
      .default = completed
    ),
    ASTDTC = if_else(is.na(ASTDT), NA_character_, as.character(ASTDT)),
    ASTDTF = if_else(is_month & !is.na(ASTDT), "D", NA_character_),
    TRTEMFL = if_else(
      !is.na(ASTDT) & !is.na(TRTSDT) & ASTDT >= TRTSDT,
      "Y",
      NA_character_
    )
  ) |>
  select(
    STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF,
    TRTSDT, TRTEMFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
