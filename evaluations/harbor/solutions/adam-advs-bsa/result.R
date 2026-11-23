# Reference solution for the yamaa benchmark adam-advs-bsa (R track).
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
  transmute(STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT)

# Height is collected once per subject, so each subject has one height
# result, whatever visit collected it; a zero result still counts.
heights <- raw |>
  filter(PARAMCD %in% "HEIGHT", !is.na(AVAL)) |>
  select(STUDYID, USUBJID, HEIGHT_VAL = AVAL)
stopifnot(!anyDuplicated(heights[c("STUDYID", "USUBJID")]))

# One body surface area record for each weight result, at the weight's
# visit, when the subject has a height result.
bsa <- raw |>
  filter(PARAMCD %in% "WEIGHT", !is.na(AVAL)) |>
  inner_join(heights, by = c("STUDYID", "USUBJID")) |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "BSA", PARAM = "Body Surface Area (m^2)",
    AVAL = sqrt(AVAL * HEIGHT_VAL / 3600),
    VISIT
  )

advs <- bind_rows(collected, bsa) |>
  arrange(USUBJID, VISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
