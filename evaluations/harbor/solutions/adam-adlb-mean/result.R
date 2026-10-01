# Reference solution for the yamaa benchmark adam-adlb-mean (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    LBSEQ = col_integer(),
    LBSTRESN = col_double(),
    .default = col_character()
  )
)

# Mean of the subject's collected values for the parameter, repeated on
# every record for that subject and parameter.
means <- lb |>
  filter(!is.na(LBSTRESN)) |>
  group_by(STUDYID, USUBJID, LBTESTCD) |>
  summarise(AVALMEAN = mean(LBSTRESN), .groups = "drop")

adlb <- lb |>
  left_join(means, by = c("STUDYID", "USUBJID", "LBTESTCD")) |>
  mutate(AVAL = LBSTRESN, PARAMCD = LBTESTCD) |>
  arrange(USUBJID, LBSEQ) |>
  select(STUDYID, USUBJID, LBSEQ, PARAMCD, AVAL, AVALMEAN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
