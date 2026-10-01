# Reference solution for the yamaa benchmark adam-advs-percentiles (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/advs_raw.csv",
  col_types = cols(AVAL = col_double(), AGE = col_integer(), .default = col_character())
)
ref <- read_csv(
  "/app/input/lms_ref.csv",
  col_types = cols(
    AGE = col_integer(), L = col_double(), M = col_double(), S = col_double(),
    .default = col_character()
  )
)

# The growth percentile against the matched L, M, and S coefficients.
advs <- raw |>
  left_join(ref, by = c("PARAMCD", "SEX", "AGE")) |>
  mutate(
    Z = case_when(
      is.na(AVAL) | is.na(L) | is.na(M) | is.na(S) | M == 0 | S == 0 ~ NA_real_,
      L == 0 ~ log(AVAL / M) / S,
      .default = ((AVAL / M) ^ L - 1) / (L * S)
    ),
    AVAL = 100 * pnorm(Z),
    PARAMCD = if_else(PARAMCD == "BMI", "BMIPCTL", "WGTPCTL"),
    PARAM = if_else(PARAMCD == "BMIPCTL", "BMI-for-Age Percentile", "Weight-for-Age Percentile")
  ) |>
  arrange(USUBJID, AVISIT, PARAMCD) |>
  select(STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
