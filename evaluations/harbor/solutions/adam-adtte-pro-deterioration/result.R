# Reference solution for the yamaa benchmark adam-adtte-pro-deterioration
# (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(lubridate, warn.conflicts = FALSE)

whole_months <- function(d1, d2) {
  m <- (year(d2) - year(d1)) * 12 + (month(d2) - month(d1))
  last <- days_in_month(d2)
  m - as.integer(day(d2) < day(d1) & day(d2) != last)
}

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(
    RANDDT = col_date(), DTHDT = col_date(), .default = col_character()
  )
)
qs <- read_csv(
  "/app/input/qs.csv",
  col_types = cols(
    QSSEQ = col_integer(), ADT = col_date(), AVAL = col_integer(),
    .default = col_character()
  )
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(
    DSSEQ = col_integer(), DSDTC = col_date(), .default = col_character()
  )
)

# Flag deteriorations: the baseline is the earliest assessment, and an
# assessment deteriorates when it is dated after the baseline and its score
# is at least 10 points below the baseline score.
qs_flagged <- qs |>
  arrange(STUDYID, USUBJID, ADT, QSSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(
    BASEDT = first(ADT),
    BASEVAL = first(AVAL),
    DETERFL = if_else(
      !is.na(ADT) & !is.na(BASEDT) & ADT > BASEDT & AVAL <= BASEVAL - 10L,
      "Y",
      NA_character_
    )
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, QSSEQ, AVISIT, ADT, AVAL, DETERFL) |>
  arrange(USUBJID, QSSEQ)

# The first deterioration per subject, earliest date then lowest sequence.
deter <- qs_flagged |>
  filter(DETERFL == "Y") |>
  arrange(STUDYID, USUBJID, ADT, QSSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, DETERDT = ADT, DETERSEQ = QSSEQ)

# The first censoring reason per subject: progression, discontinuation, or
# withdrawal are censoring reasons, never events.
reason <- ds |>
  arrange(STUDYID, USUBJID, DSDTC, DSSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, CENSORRSNDT = DSDTC, CENSORRSN = DSDECOD)

# The latest assessment dated on or before the censoring reason, with the
# latest sequence breaking a tied date.
censor_qs <- qs_flagged |>
  inner_join(reason, by = c("STUDYID", "USUBJID")) |>
  filter(!is.na(ADT), ADT <= CENSORRSNDT) |>
  group_by(STUDYID, USUBJID) |>
  summarize(
    LASTQSLE = max(ADT),
    LASTQSSEQ = max(QSSEQ[ADT == max(ADT)]),
    .groups = "drop"
  )

adtte <- adsl |>
  left_join(deter, by = c("STUDYID", "USUBJID")) |>
  left_join(reason, by = c("STUDYID", "USUBJID")) |>
  left_join(censor_qs, by = c("STUDYID", "USUBJID")) |>
  mutate(
    STARTDT = RANDDT,
    # Deterioration wins over death on the same date.
    EVENTDT = case_when(
      !is.na(DETERDT) & (is.na(DTHDT) | DETERDT <= DTHDT) ~ DETERDT,
      !is.na(DTHDT) ~ DTHDT,
      .default = as.Date(NA)
    ),
    CENSORDT = coalesce(LASTQSLE, STARTDT),
    ADT = coalesce(EVENTDT, CENSORDT),
    AVAL = whole_months(STARTDT, ADT),
    CNSR = if_else(!is.na(EVENTDT), 0L, 1L),
    EVNTDESC = case_when(
      !is.na(DETERDT) & (is.na(DTHDT) | DETERDT <= DTHDT) ~ "PRO DETERIORATION",
      !is.na(DTHDT) ~ "DEATH",
      .default = "CENSORED"
    ),
    CNSDTDSC = case_when(
      !is.na(EVENTDT) ~ NA_character_,
      .default = coalesce(CENSORRSN, "STUDY COMPLETION")
    ),
    SRCDOM = case_when(
      !is.na(DETERDT) & (is.na(DTHDT) | DETERDT <= DTHDT) ~ "QS",
      !is.na(DTHDT) ~ "ADSL",
      is.na(LASTQSLE) ~ "ADSL",
      .default = "QS"
    ),
    SRCVAR = case_when(
      !is.na(DETERDT) & (is.na(DTHDT) | DETERDT <= DTHDT) ~ "ADT",
      !is.na(DTHDT) ~ "DTHDT",
      is.na(LASTQSLE) ~ "RANDDT",
      .default = "ADT"
    ),
    SRCSEQ = case_when(
      !is.na(DETERDT) & (is.na(DTHDT) | DETERDT <= DTHDT) ~ DETERSEQ,
      is.na(EVENTDT) & !is.na(LASTQSLE) ~ LASTQSSEQ,
      .default = NA_integer_
    ),
    PARAMCD = "TTDGHS",
    PARAM = "Time to Deterioration in Global Health Status"
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR,
    EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ
  ) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(qs_flagged, "/app/output/qs_flagged.csv", na = "")
write_csv(adtte, "/app/output/adtte.csv", na = "")
