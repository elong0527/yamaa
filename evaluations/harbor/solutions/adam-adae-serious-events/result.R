# Reference solution for the yamaa benchmark adam-adae-serious-events (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

# Only serious events are kept; AESER is Y on every row.
adae <- ae |>
  filter(AESER %in% "Y") |>
  select(STUDYID, USUBJID, AESEQ, AEDECOD, AESER)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
