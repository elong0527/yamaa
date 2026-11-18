# Reference solution for the yamaa benchmark adam-adlb-mean (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
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

collected <- lb |>
  transmute(
    STUDYID, USUBJID, LBSEQ,
    PARAMCD = LBTESTCD,
    AVAL = LBSTRESN,
    DTYPE = NA_character_
  )

# One average record per subject and parameter with a non-missing result.
averages <- lb |>
  filter(!is.na(LBSTRESN)) |>
  group_by(STUDYID, USUBJID, PARAMCD = LBTESTCD) |>
  summarise(AVAL = mean(LBSTRESN), .groups = "drop") |>
  mutate(LBSEQ = NA_integer_, DTYPE = "AVERAGE")

# Collected records in sequence order, then the average, within parameter.
adlb <- bind_rows(collected, averages) |>
  arrange(STUDYID, USUBJID, PARAMCD, !is.na(DTYPE), LBSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(ASEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, ASEQ) |>
  select(STUDYID, USUBJID, ASEQ, LBSEQ, PARAMCD, AVAL, DTYPE)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
