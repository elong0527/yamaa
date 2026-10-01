# Reference solution for the yamaa benchmark adam-adlb-reported-precision (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    AVAL = col_double(),
    ANRLO = col_double(),
    .default = col_character()
  )
)

# The result as a multiple of the lower limit, rounded once to four places
# with an exact half away from zero.
adlb <- lb |>
  mutate(
    ratio = AVAL / ANRLO,
    R2ANRLO = case_when(
      is.na(AVAL) | is.na(ANRLO) | ANRLO == 0 ~ NA_real_,
      ratio >= 0 ~ floor(ratio * 10000 + 0.5) / 10000,
      .default = ceiling(ratio * 10000 - 0.5) / 10000
    )
  ) |>
  arrange(USUBJID) |>
  select(STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
