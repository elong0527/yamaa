# Reference solution for the yamaa benchmark adam-adqs-screening-baseline (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character()),
  na = character(),
  progress = FALSE
) |>
  mutate(TRTSDT = as.Date(na_if(TRTSDT, ""))) |>
  select(STUDYID, USUBJID, TRTSDT)

qs <- read_csv(
  "/app/input/qs.csv",
  col_types = cols(.default = col_character()),
  na = character(),
  progress = FALSE
) |>
  mutate(
    QSSTRESN = as.numeric(na_if(QSSTRESN, "")),
    VISITNUM = as.numeric(VISITNUM),
    QSDTC = na_if(QSDTC, "")
  )

# One record per subject, parameter and visit, with the subject's treatment
# start date. The rating is the collected result, and it is missing when the
# questionnaire was not done. ADT is the collected date, not-done records
# included.
adqs <- qs |>
  left_join(adsl, by = c("STUDYID", "USUBJID")) |>
  transmute(
    STUDYID,
    USUBJID,
    PARAMCD = QSTESTCD,
    PARAM = QSTEST,
    VISITNUM,
    VISIT,
    ADT = as.Date(substr(QSDTC, 1, 10)),
    TRTSDT,
    AVAL = QSSTRESN
  )

# A record can be the baseline only when it has a rating. Cycle 1 Day 1 counts
# when dated on or before the treatment start date; otherwise screening counts.
has_rating <- !is.na(adqs$AVAL)
c1d1_ok <- adqs$VISIT == "CYCLE 1 DAY 1" & has_rating &
  !is.na(adqs$ADT) & !is.na(adqs$TRTSDT) & adqs$ADT <= adqs$TRTSDT
scr_ok <- adqs$VISIT == "SCREENING" & has_rating

adqs <- adqs |>
  mutate(.c1d1_ok = c1d1_ok, .scr_ok = scr_ok) |>
  group_by(USUBJID, PARAMCD) |>
  mutate(
    .use_c1d1 = any(.c1d1_ok),
    ABLFL = case_when(
      .use_c1d1 & .c1d1_ok ~ "Y",
      !.use_c1d1 & .scr_ok ~ "Y",
      TRUE ~ NA_character_
    ),
    SCBLFL = if_else(!is.na(ABLFL) & VISIT == "SCREENING", "Y", NA_character_),
    BASE = if (any(!is.na(ABLFL))) AVAL[which(!is.na(ABLFL))[1]] else NA_real_
  ) |>
  ungroup() |>
  mutate(
    CHG = if_else(
      !is.na(ADT) & !is.na(TRTSDT) & ADT > TRTSDT &
        !is.na(AVAL) & !is.na(BASE),
      AVAL - BASE,
      NA_real_
    )
  ) |>
  arrange(USUBJID, PARAMCD, VISITNUM) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, VISITNUM, VISIT, ADT, TRTSDT,
    AVAL, ABLFL, SCBLFL, BASE, CHG
  )

dir.create("/app/output", showWarnings = FALSE, recursive = TRUE)
write_csv(adqs, "/app/output/adqs.csv", na = "")
