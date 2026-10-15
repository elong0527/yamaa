# Reference solution for the yamaa benchmark adam-adae-partial-times (R track).
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

# Complete a collected value by its length: the missing parts take their
# earliest (start) or latest (end) values. A value without its day, or no
# value at all, gives no analysis time.
complete_time <- function(x, last) {
  n <- nchar(x)
  case_when(
    n == 19 ~ x,
    n == 16 ~ paste0(x, if (last) ":59" else ":00"),
    n == 13 ~ paste0(x, if (last) ":59:59" else ":00:00"),
    n == 10 ~ paste0(x, if (last) "T23:59:59" else "T00:00:00"),
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
    ASTDTM = complete_time(AESTDTC, last = FALSE),
    ASTTMF = time_flag(AESTDTC),
    AENDTM = complete_time(AEENDTC, last = TRUE),
    AENTMF = time_flag(AEENDTC)
  ) |>
  select(
    STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDTM, ASTTMF,
    AEENDTC, AENDTM, AENTMF
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
