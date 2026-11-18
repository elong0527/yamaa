# Reference solution for the yamaa benchmark adam-adppk-dose-intervals (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(stringr)
})

ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(.default = col_character()),
  na = c("", "NA")
)

has_time <- function(x) !is.na(x) & str_detect(x, "T[0-9]{2}")

to_datetime <- function(x) {
  date <- str_sub(x, 1, 10)
  time <- if_else(has_time(x), str_sub(x, 12), "00:00")
  time <- case_when(
    str_detect(time, "^[0-9]{2}$") ~ paste0(time, ":00:00"),
    str_detect(time, "^[0-9]{2}:[0-9]{2}$") ~ paste0(time, ":00"),
    TRUE ~ str_sub(time, 1, 8)
  )
  as.POSIXct(
    if_else(is.na(x), NA_character_, paste(date, time)),
    format = "%Y-%m-%d %H:%M:%S",
    tz = "UTC"
  )
}

adppk <- ex %>%
  mutate(
    EXSEQ = as.numeric(EXSEQ),
    EXDOSE = as.numeric(EXDOSE)
  ) %>%
  filter(!is.na(EXDOSE), EXDOSE > 0) %>%
  mutate(
    start = to_datetime(EXSTDTC),
    end_time = has_time(EXENDTC),
    end = to_datetime(EXENDTC),
    interval = case_when(
      EXDOSFRQ == "QD" ~ 24,
      EXDOSFRQ == "BID" ~ 12,
      TRUE ~ NA_real_
    ),
    span = as.numeric(difftime(end, start, units = "secs")),
    count = if_else(
      end_time,
      floor(span / (interval * 3600)),
      ceiling((span + 86400) / (interval * 3600)) - 1
    ),
    ADTM = format(start, "%Y-%m-%dT%H:%M:%S"),
    ATMF = if_else(has_time(EXSTDTC), NA_character_, "H"),
    EVID = 1,
    AMT = EXDOSE,
    ADDL = if_else(is.na(count) | count < 0, 0, count),
    II = if_else(ADDL > 0, interval, 0)
  ) %>%
  select(STUDYID, USUBJID, EXSEQ, ADTM, ATMF, EVID, AMT, II, ADDL)

write_csv(adppk, "/app/output/adppk.csv", na = "")
