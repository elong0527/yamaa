# Reference solution for the yamaa benchmark adam-adtr-nadir (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adtr_raw.csv",
  col_types = cols(
    AVISITN = col_integer(), ADT = col_date(), AVAL = col_double(),
    NMEAS = col_integer(), NTARGET = col_integer(),
    .default = col_character()
  )
)

# Complete dated assessments carry the nadir candidates; a complete
# assessment with no date never counts toward any nadir, its own
# included.
complete <- raw |>
  filter(ANL01FL == "Y", !is.na(ADT), !is.na(AVAL)) |>
  select(USUBJID, ADT, AVAL)

# An incomplete current assessment keeps the nadir set by an earlier
# complete one, while a current record with no date has none.
adtr <- raw |>
  rowwise() |>
  mutate(
    NADIR = {
      cands <- complete$AVAL[complete$USUBJID == USUBJID & complete$ADT <= ADT]
      if (is.na(ADT) || length(cands) == 0L) NA_real_ else min(cands)
    }
  ) |>
  ungroup() |>
  select(
    STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM,
    AVAL, NMEAS, NTARGET, ANL01FL, NADIR
  ) |>
  arrange(USUBJID, AVISITN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtr, "/app/output/adtr.csv", na = "")
