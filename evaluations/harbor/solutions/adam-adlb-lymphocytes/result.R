# Reference solution for the yamaa benchmark adam-adlb-lymphocytes (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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

# One LYMPH record per subject and visit with both WBC and LYMLE and no
# LYMPH yet: WBC times LYMLE.
wbc <- raw |>
  filter(PARAMCD %in% "WBC") |>
  select(USUBJID, VISIT, WBCVAL = AVAL)
lymle <- raw |>
  filter(PARAMCD %in% "LYMLE") |>
  select(USUBJID, VISIT, LYMLEVAL = AVAL)
existing <- raw |>
  filter(PARAMCD %in% "LYMPH") |>
  distinct(USUBJID, VISIT) |>
  mutate(HAS_LYMPH = TRUE)

calc <- wbc |>
  inner_join(lymle, by = c("USUBJID", "VISIT")) |>
  left_join(existing, by = c("USUBJID", "VISIT")) |>
  filter(!is.na(WBCVAL), !is.na(LYMLEVAL), is.na(HAS_LYMPH)) |>
  mutate(
    PARAMCD = "LYMPH",
    AVAL = WBCVAL * LYMLEVAL,
    PARAM = "Lymphocytes Abs (10^9/L)",
    DTYPE = "CALCULATION"
  ) |>
  select(USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE)

adlb <- bind_rows(collected, calc)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
