# Reference solution for the yamaa benchmark adam-adlb-shift-criteria (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adlb_raw.csv",
  col_types = cols(
    ASEQ = col_integer(),
    AVAL = col_double(),
    ANRLO = col_double(),
    ANRHI = col_double(),
    .default = col_character()
  )
)

# The record's own mark: LOW below the lower limit, HIGH above the upper
# limit, NORMAL between them limits included. No value when the result or
# either limit is missing.
based <- raw |>
  mutate(
    ANRIND = case_when(
      is.na(AVAL) | is.na(ANRLO) | is.na(ANRHI) ~ NA_character_,
      AVAL < ANRLO ~ "LOW",
      AVAL > ANRHI ~ "HIGH",
      .default = "NORMAL"
    )
  )

# BASE repeats the flagged baseline record's value; BNRIND repeats its
# mark. Both have no value when no record carries the flag.
baselines <- based |>
  filter(ABLFL %in% "Y") |>
  select(STUDYID, USUBJID, PARAMCD, BASE = AVAL, BNRIND = ANRIND)

with_base <- based |>
  left_join(baselines, by = c("STUDYID", "USUBJID", "PARAMCD"))

# SHIFT1 joins the baseline mark and the record's own mark; R2BASE is the
# record as a multiple of the baseline; CRIT1 assesses greater than three
# times the upper limit.
adlb <- with_base |>
  mutate(
    SHIFT1 = if_else(
      is.na(BNRIND) | is.na(ANRIND),
      NA_character_,
      paste(BNRIND, "to", ANRIND)
    ),
    R2BASE = if_else(
      is.na(AVAL) | is.na(BASE) | BASE == 0,
      NA_real_,
      AVAL / BASE
    ),
    CRITLIM = 3 * ANRHI,
    CRIT1 = if_else(
      !is.na(AVAL) & !is.na(CRITLIM),
      "Result greater than 3 x ULN",
      NA_character_
    ),
    CRIT1FL = case_when(
      is.na(AVAL) | is.na(CRITLIM) ~ NA_character_,
      AVAL > CRITLIM ~ "Y",
      .default = "N"
    )
  ) |>
  arrange(USUBJID, ASEQ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI,
    ANRIND, ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
