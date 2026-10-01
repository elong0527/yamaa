# Reference solution for the yamaa benchmark adam-adsl-duration (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(STDT = col_date(), ENDT = col_date(), .default = col_character())
)

add_months <- function(day, n) {
  total <- as.integer(format(day, "%m")) - 1L + n
  year <- as.integer(format(day, "%Y")) + total %/% 12L
  month <- total %% 12L + 1L
  last <- days_in_month(make_date(year, month, 1L))
  pmin_day <- min(as.integer(format(day, "%d")), last)
  make_date(year, month, pmin_day)
}

# Monthly anniversaries of the start on or before the end, keeping the
# start day or the month's last day when the month is too short; negated
# with the dates exchanged when the end falls before the start.
whole_months <- function(start, end) {
  if (is.na(start) || is.na(end)) {
    return(NA_integer_)
  }
  if (end < start) {
    return(-whole_months(end, start))
  }
  months <- (as.integer(format(end, "%Y")) - as.integer(format(start, "%Y"))) * 12L +
    (as.integer(format(end, "%m")) - as.integer(format(start, "%m")))
  while (months > 0L && add_months(start, months) > end) {
    months <- months - 1L
  }
  while (add_months(start, months + 1L) <= end) {
    months <- months + 1L
  }
  months
}

# Whole seven-day blocks from start to end, dropping any leftover partial
# week; negated with the dates exchanged when the end falls before the start.
whole_weeks <- function(start, end) {
  if (is.na(start) || is.na(end)) {
    return(NA_integer_)
  }
  if (end < start) {
    return(-whole_weeks(end, start))
  }
  as.integer(trunc(as.numeric(end - start) / 7))
}

adsl <- dm |>
  rowwise() |>
  mutate(DURW = whole_weeks(STDT, ENDT), DURM = whole_months(STDT, ENDT)) |>
  ungroup() |>
  select(STUDYID, USUBJID, STDT, ENDT, DURW, DURM)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
