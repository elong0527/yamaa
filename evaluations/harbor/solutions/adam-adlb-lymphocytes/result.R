# Reference solution for the yamaa benchmark adam-adlb-lymphocytes (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adlb.csv",
  col_types = cols(AVAL = col_double(), .default = col_character())
)

# Keep every collected record unchanged, with DTYPE empty.
collected <- raw |>
  select(USUBJID, PARAMCD, AVAL, PARAM, VISIT) |>
  mutate(DTYPE = NA_character_)

# Differential fraction code -> absolute code and parameter name.
differentials <- list(
  LYMLE = c("LYMPH", "Lymphocytes Abs (10^9/L)"),
  NEUTLE = c("NEUT", "Neutrophils Abs (10^9/L)"),
  MONOLE = c("MONO", "Monocytes Abs (10^9/L)"),
  EOSLE = c("EOS", "Eosinophils Abs (10^9/L)"),
  BASOLE = c("BASO", "Basophils Abs (10^9/L)")
)

wbc <- raw |>
  filter(PARAMCD == "WBC") |>
  select(USUBJID, VISIT, WBCVAL = AVAL)

# One absolute record per subject and visit for each differential
# fraction present with a WBC count and no absolute record yet.
calc <- bind_rows(lapply(names(differentials), function(frac_code) {
  abs_code <- differentials[[frac_code]][1]
  abs_param <- differentials[[frac_code]][2]
  frac <- raw |>
    filter(PARAMCD == frac_code) |>
    select(USUBJID, VISIT, FRACVAL = AVAL)
  existing <- raw |>
    filter(PARAMCD == abs_code) |>
    distinct(USUBJID, VISIT) |>
    mutate(HAS_ABS = TRUE)
  wbc |>
    inner_join(frac, by = c("USUBJID", "VISIT")) |>
    left_join(existing, by = c("USUBJID", "VISIT")) |>
    filter(!is.na(WBCVAL), !is.na(FRACVAL), is.na(HAS_ABS)) |>
    mutate(
      PARAMCD = abs_code,
      AVAL = WBCVAL * FRACVAL,
      PARAM = abs_param,
      DTYPE = "CALCULATION"
    ) |>
    select(USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE)
}))

adlb <- bind_rows(collected, calc)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
