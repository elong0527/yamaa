# Reference solution for the yamaa benchmark adam-adrs-measurable-disease (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
tu <- read_csv(
  "/app/input/tu.csv",
  col_types = cols(TUSEQ = col_integer(), .default = col_character())
)

# Target disease at screening: a screening tumor identification record
# whose result is target. Later target records do not count.
target_subjects <- tu |>
  filter(
    TUTESTCD %in% "TUMIDENT",
    VISIT %in% "SCREENING",
    TUSTRESC %in% "TARGET"
  ) |>
  distinct(USUBJID)

adrs <- adsl |>
  mutate(
    PARAMCD = "MDIS",
    PARAM = "Measurable Disease at Baseline",
    AVALC = if_else(USUBJID %in% target_subjects$USUBJID, "Y", "N"),
    AVAL = if_else(AVALC == "Y", 1L, 0L)
  ) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adrs, "/app/output/adrs.csv", na = "")
