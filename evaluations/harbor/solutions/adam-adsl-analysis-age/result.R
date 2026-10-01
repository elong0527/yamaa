# Reference solution for the yamaa benchmark adam-adsl-analysis-age (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(BRTHDT = col_date(), RANDDT = col_date(), .default = col_character())
)

is_leap <- function(year) {
  year %% 4 == 0 & (year %% 100 != 0 | year %% 400 == 0)
}

# Yearly anniversaries of the birth date on or before randomization; a
# February 29 birthday falls on February 28 in common years, and a date
# before birth gives the negated count with the dates exchanged.
whole_years <- function(start, end) {
  if (is.na(start) || is.na(end)) {
    return(NA_integer_)
  }
  if (end < start) {
    return(-whole_years(end, start))
  }
  years <- as.integer(format(end, "%Y")) - as.integer(format(start, "%Y"))
  month <- as.integer(format(start, "%m"))
  day <- as.integer(format(start, "%d"))
  if (month == 2L && day == 29L && !is_leap(as.integer(format(end, "%Y")))) {
    day <- 28L
  }
  end_month <- as.integer(format(end, "%m"))
  end_day <- as.integer(format(end, "%d"))
  if (end_month < month || (end_month == month && end_day < day)) {
    years <- years - 1L
  }
  years
}

adsl <- dm |>
  rowwise() |>
  mutate(AAGE = whole_years(BRTHDT, RANDDT), AAGEU = "YEARS") |>
  ungroup() |>
  select(STUDYID, USUBJID, BRTHDT, RANDDT, AAGE, AAGEU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
