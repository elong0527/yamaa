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
    AP01SDT = col_date(), AP02SDT = col_date(),
    AP01SDTM = col_datetime(), AP02SDTM = col_datetime(),
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

# The latest period whose start is at or before the record. A record
# with a time is compared with the period start datetimes, a date-only
# record with the period start dates; a missing start never matches,
# so a subject who never started period 2 stays in period 1.
advs <- collected |>
  left_join(adsl, by = c("STUDYID", "USUBJID")) |>
  mutate(
    APERIOD = case_when(
      !is.na(moment) & !is.na(AP02SDTM) & moment >= AP02SDTM ~ 2L,
      is.na(moment) & !is.na(ADT) & !is.na(AP02SDT) & ADT >= AP02SDT ~ 2L,
      !is.na(moment) & !is.na(AP01SDTM) & moment >= AP01SDTM ~ 1L,
      is.na(moment) & !is.na(ADT) & !is.na(AP01SDT) & ADT >= AP01SDT ~ 1L,
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
