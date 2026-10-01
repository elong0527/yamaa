# Reference solution for the yamaa benchmark adam-adlb-ordered-sum (R track).
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

# Keep every collected component record; DTYPE has no value on these.
components <- lb |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = LBTESTCD,
    PARAM = LBTEST,
    AVISIT = VISIT,
    AVAL = LBSTRESN,
    DTYPE = NA_character_
  )

# One total record per subject and visit: the sum of the collected
# component results in laboratory sequence order. No value, rather than
# zero, when none of them has a collected result.
ordered_sum <- function(x) {
  vals <- x[!is.na(x)]
  if (length(vals) == 0) {
    return(NA_real_)
  }
  total <- vals[[1]]
  if (length(vals) > 1) {
    for (v in vals[-1]) {
      total <- total + v
    }
  }
  total
}

totals <- lb |>
  arrange(USUBJID, VISIT, LBSEQ) |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(
    AVAL = ordered_sum(LBSTRESN),
    .groups = "drop"
  ) |>
  mutate(
    PARAMCD = "TOTAL",
    PARAM = "Total of Components",
    AVISIT = VISIT,
    DTYPE = "CALCULATION"
  ) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE)

adlb <- bind_rows(components, totals)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
