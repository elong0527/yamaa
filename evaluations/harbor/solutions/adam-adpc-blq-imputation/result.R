# Reference solution for the yamaa benchmark adam-adpc-blq-imputation (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

pc <- read_csv(
  "/app/input/pc.csv",
  col_types = cols(
    PCSEQ = col_integer(),
    PCSTRESN = col_double(),
    PCTPTNUM = col_double(),
    .default = col_character()
  ),
  na = ""
)

# The first and last quantified time points of each profile. A result
# below the limit and a sample not done have no number, so neither moves
# the quantified span.
span <- pc |>
  filter(!is.na(PCSTRESN)) |>
  group_by(STUDYID, USUBJID, PCTESTCD, PCTPTREF) |>
  summarise(
    FIRSTQ = min(PCTPTNUM),
    LASTQ = max(PCTPTNUM),
    .groups = "drop"
  )

based <- pc |>
  left_join(
    span,
    by = c("STUDYID", "USUBJID", "PCTESTCD", "PCTPTREF")
  )

# A quantified result as recorded; a below-limit result is 0 before the
# first or after the last quantified result of its profile (and in a
# profile with no quantified result), and has no value between them. A
# sample not done has no value.
adpc <- based |>
  mutate(
    AVAL = case_when(
      !is.na(PCSTRESN) ~ PCSTRESN,
      !is.na(PCSTRESC) & startsWith(PCSTRESC, "<") &
        (is.na(FIRSTQ) | PCTPTNUM < FIRSTQ | PCTPTNUM > LASTQ) ~ 0,
      .default = NA_real_
    )
  ) |>
  select(
    STUDYID, USUBJID, PCSEQ,
    PARAMCD = PCTESTCD, ATPTREF = PCTPTREF, ATPT = PCTPT,
    ATPTN = PCTPTNUM, AVAL, AVALU = PCSTRESU
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpc, "/app/output/adpc.csv", na = "")
