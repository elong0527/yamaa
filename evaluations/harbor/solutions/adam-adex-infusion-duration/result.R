# Reference solution for the yamaa benchmark adam-adex-infusion-duration (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)
library(stringr)

ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)

# A date alone has no time of day, and none is assumed; a time without
# seconds counts as second zero.
moment <- function(x) {
  full <- if_else(nchar(x) == 16, paste0(x, ":00"), x)
  if_else(!is.na(x) & nchar(x) > 10, ymd_hms(full, tz = "UTC", quiet = TRUE), as.POSIXct(NA, tz = "UTC"))
}

# An ISO 8601 duration of days, hours, minutes and seconds, with at least one
# part; anything else counts as not collected.
collected_minutes <- function(x) {
  pattern <- "^P(?:(\\d+)D)?(?:T(?:(\\d+)H)?(?:(\\d+)M)?(?:(\\d+(?:\\.\\d+)?)S)?)?$"
  parts <- str_match(x, pattern)
  number <- function(i) coalesce(as.numeric(parts[, i]), 0)
  found <- !is.na(parts[, 1]) & rowSums(!is.na(parts[, 2:5, drop = FALSE])) > 0 & !str_ends(coalesce(x, ""), "T")
  if_else(found, number(2) * 1440 + number(3) * 60 + number(4) + number(5) / 60, NA_real_)
}

adex <- ex |>
  mutate(
    CALCDUR = as.numeric(difftime(moment(EXENDTC), moment(EXSTDTC), units = "mins")),
    AVAL = coalesce(collected_minutes(EXDUR), CALCDUR),
    PARAMCD = "INFDUR",
    PARAM = "Infusion Duration (min)",
    AVALU = "min"
  ) |>
  select(STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU, CALCDUR)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adex, "/app/output/adex.csv", na = "")
