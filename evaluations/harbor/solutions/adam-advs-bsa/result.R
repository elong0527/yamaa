# Reference solution for the yamaa benchmark adam-advs-bsa (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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

# The visit's height and weight results; a zero result still counts.
heights <- raw |>
  filter(PARAMCD %in% "HEIGHT", !is.na(AVAL)) |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(HEIGHT_VAL = first(AVAL), .groups = "drop")

weights <- raw |>
  filter(PARAMCD %in% "WEIGHT", !is.na(AVAL)) |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(WEIGHT_VAL = first(AVAL), .groups = "drop")

# One body surface area record for each visit with both results.
bsa <- heights |>
  inner_join(weights, by = c("STUDYID", "USUBJID", "VISIT")) |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "BSA", PARAM = "Body Surface Area (m^2)",
    AVAL = sqrt(HEIGHT_VAL * WEIGHT_VAL / 3600),
    VISIT, DTYPE = "CALCULATION"
  )

advs <- bind_rows(collected, bsa) |>
  arrange(USUBJID, VISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
