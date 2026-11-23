# Reference solution for the yamaa benchmark
# adam-advs-crossover-period-records (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(readr)
library(stringr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(), VSSTRESN = col_double(),
    .default = col_character()
  )
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(
    AP01SDT = col_date(), AP01EDT = col_date(),
    AP02SDT = col_date(), AP02EDT = col_date(),
    AP01SDTM = col_datetime(), AP01EDTM = col_datetime(),
    AP02SDTM = col_datetime(), AP02EDTM = col_datetime(),
    .default = col_character()
  )
)

# The collection date is the date part of the collection datetime;
# the datetime itself exists only when a time was collected. A time
# may be written with or without seconds, so both forms are tried.
collected <- vs |>
  mutate(
    ADT = as.Date(str_sub(VSDTC, 1, 10)),
    parsed_hms = suppressWarnings(ymd_hms(VSDTC, quiet = TRUE, tz = "UTC")),
    parsed_hm = suppressWarnings(ymd_hm(VSDTC, quiet = TRUE, tz = "UTC")),
    moment = case_when(
      !is.na(VSDTC) & str_detect(VSDTC, "T") ~
        coalesce(parsed_hms, parsed_hm),
      .default = as.POSIXct(NA, tz = "UTC")
    ),
    ADTM = format(moment, "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  )

# Whether a record lies within a period's start and end, both included.
# Each limit is compared by time when both the record and the limit have
# one, and by date otherwise; a missing date or limit never matches.
within <- function(adt, moment, sdt, sdtm, edt, edtm) {
  after_start <- if_else(
    !is.na(moment) & !is.na(sdtm), moment >= sdtm, adt >= sdt
  )
  before_end <- if_else(
    !is.na(moment) & !is.na(edtm), moment <= edtm, adt <= edt
  )
  coalesce(after_start & before_end, FALSE)
}

# The later period wins when a record lies within both.
advs <- collected |>
  left_join(adsl, by = c("STUDYID", "USUBJID")) |>
  mutate(
    APERIOD = case_when(
      within(ADT, moment, AP02SDT, AP02SDTM, AP02EDT, AP02EDTM) ~ 2L,
      within(ADT, moment, AP01SDT, AP01SDTM, AP01EDT, AP01EDTM) ~ 1L,
      .default = NA_integer_
    ),
    PARAMCD = VSTESTCD,
    AVAL = VSSTRESN,
    TRTA = case_when(
      APERIOD %in% 1L ~ TRT01A,
      APERIOD %in% 2L ~ TRT02A,
      .default = NA_character_
    )
  ) |>
  select(
    STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
