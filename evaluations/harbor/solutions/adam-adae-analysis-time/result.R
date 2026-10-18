# Reference solution for the yamaa benchmark adam-adae-analysis-time (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

# Complete a collected start by its length: the missing parts of the time
# take their earliest values. A start without its day, or no start at all,
# gives no analysis time.
complete_start <- function(x) {
  n <- nchar(x)
  case_when(
    n == 19 ~ x,
    n == 16 ~ paste0(x, ":00"),
    n == 13 ~ paste0(x, ":00:00"),
    n == 10 ~ paste0(x, "T00:00:00"),
    TRUE ~ NA_character_
  )
}

# The flag names the coarsest part of the time that was supplied.
time_flag <- function(x) {
  n <- nchar(x)
  case_when(
    n == 16 ~ "S",
    n == 13 ~ "M",
    n == 10 ~ "H",
    TRUE ~ NA_character_
  )
}

adae <- ae |>
  mutate(
    ASTDTM = complete_start(AESTDTC),
    ASTDT = substr(ASTDTM, 1, 10),
    ASTTM = as.integer(substr(ASTDTM, 12, 13)) * 3600L +
      as.integer(substr(ASTDTM, 15, 16)) * 60L +
      as.integer(substr(ASTDTM, 18, 19)),
    ASTTMF = time_flag(AESTDTC)
  ) |>
  select(STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, ASTDT, ASTTM, ASTTMF)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
