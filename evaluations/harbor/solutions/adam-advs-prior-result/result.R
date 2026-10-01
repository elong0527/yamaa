# Reference solution for the yamaa benchmark adam-advs-prior-result (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(), AVISITN = col_integer(),
    .default = col_character()
  )
)

# Within a series, rows are ordered by visit number with missing numbers
# last, keeping the collected order for ties. The look-back skips blank
# results but never crosses into another series.
ordered <- vs |>
  arrange(STUDYID, USUBJID, SERIES, is.na(AVISITN), AVISITN, VSSEQ)

# Forward-fill the ordered results, then lag by one so the row's own
# value never counts.
prev <- ordered |>
  group_by(STUDYID, USUBJID, SERIES) |>
  fill(AVALC, .direction = "down") |>
  mutate(PREVAVALC = lag(AVALC)) |>
  ungroup() |>
  select(STUDYID, USUBJID, VSSEQ, PREVAVALC)

advs <- vs |>
  left_join(prev, by = c("STUDYID", "USUBJID", "VSSEQ")) |>
  arrange(USUBJID, VSSEQ) |>
  select(STUDYID, USUBJID, VSSEQ, SERIES, AVISITN, AVALC, PREVAVALC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
