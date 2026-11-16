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

# A quantified result as recorded and a below-limit result 0; a sample
# not done has no value. A below-limit result between the first and the
# last quantified result of its profile is flagged for exclusion.
adpc <- based |>
  mutate(
    BLQ = !is.na(PCSTRESC) & startsWith(PCSTRESC, "<"),
    AVAL = case_when(
      !is.na(PCSTRESN) ~ PCSTRESN,
      BLQ ~ 0,
      .default = NA_real_
    ),
    NCAXFL = if_else(
      BLQ & !is.na(FIRSTQ) & PCTPTNUM > FIRSTQ & PCTPTNUM < LASTQ,
      "Y", NA_character_
    )
  ) |>
  select(
    STUDYID, USUBJID, PCSEQ,
    PARAMCD = PCTESTCD, ATPTREF = PCTPTREF, ATPT = PCTPT,
    ATPTN = PCTPTNUM, AVAL, AVALU = PCSTRESU, NCAXFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpc, "/app/output/adpc.csv", na = "")
