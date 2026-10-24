# Reference solution for the yamaa benchmark adam-adis-log-titer (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

is_in <- read_csv(
  "/app/input/is.csv",
  col_types = cols(
    ISSTRESN = col_double(),
    ISLLOQ = col_double(),
    .default = col_character()
  )
)

# A titer below the lower limit of quantitation gives no record; a record
# whose titer has no value stays.
adis <- is_in |>
  filter(is.na(ISSTRESN) | ISSTRESN >= ISLLOQ) |>
  mutate(
    PARAMCD = "NABLOG10",
    AVISIT = VISIT,
    AVAL = log10(ISSTRESN)
  ) |>
  select(STUDYID, USUBJID, PARAMCD, AVISIT, AVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adis, "/app/output/adis.csv", na = "")
