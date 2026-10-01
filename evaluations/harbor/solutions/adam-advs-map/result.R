# Reference solution for the yamaa benchmark adam-advs-map (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/advs.csv",
  col_types = cols(AVAL = col_double(), .default = col_character())
)

# Every collected record stays in place.
collected <- raw |>
  transmute(STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE = NA_character_)

# The visit's systolic and diastolic results.
sysbp <- raw |>
  filter(PARAMCD %in% "SYSBP", !is.na(AVAL)) |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(SYSBP_VAL = first(AVAL), .groups = "drop")

diabp <- raw |>
  filter(PARAMCD %in% "DIABP", !is.na(AVAL)) |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(DIABP_VAL = first(AVAL), .groups = "drop")

# One mean arterial pressure record for each visit with both results,
# counting the diastolic pressure twice.
map_records <- sysbp |>
  inner_join(diabp, by = c("STUDYID", "USUBJID", "VISIT")) |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "MAP", PARAM = "Mean Arterial Pressure (mmHg)",
    AVAL = (SYSBP_VAL + 2 * DIABP_VAL) / 3,
    VISIT, DTYPE = "CALCULATION"
  )

advs <- bind_rows(collected, map_records) |>
  arrange(USUBJID, VISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
