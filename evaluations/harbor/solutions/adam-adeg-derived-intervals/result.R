# Reference solution for the yamaa benchmark adam-adeg-derived-intervals (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

adeg <- read_csv(
  "/app/input/adeg.csv",
  col_types = cols(AVAL = col_double(), .default = col_character())
)

wide <- adeg |>
  select(STUDYID, USUBJID, AVISIT, PARAMCD, AVAL) |>
  pivot_wider(names_from = PARAMCD, values_from = AVAL)

# A QTc record needs both QT and RR; RR in seconds is RR in ms over 1000.
qtc <- wide |>
  filter(!is.na(QT), !is.na(RR)) |>
  mutate(
    RRSEC = RR / 1000,
    QTCBR = QT / sqrt(RRSEC),
    QTCFR = QT / (RRSEC^(1 / 3))
  )

qtcbr <- qtc |>
  transmute(
    STUDYID, USUBJID, AVISIT,
    PARAMCD = "QTCBR",
    PARAM = "QTcB - Bazett's Correction Formula Rederived (ms)",
    AVAL = QTCBR,
    AVALU = "ms"
  )

qtcfr <- qtc |>
  transmute(
    STUDYID, USUBJID, AVISIT,
    PARAMCD = "QTCFR",
    PARAM = "QTcF - Fridericia's Correction Formula Rederived (ms)",
    AVAL = QTCFR,
    AVALU = "ms"
  )

# An RRR record needs a present, nonzero heart rate.
rrr <- wide |>
  filter(!is.na(HR), HR != 0) |>
  transmute(
    STUDYID, USUBJID, AVISIT,
    PARAMCD = "RRR",
    PARAM = "RR Duration Rederived (ms)",
    AVAL = 60000 / HR,
    AVALU = "ms"
  )

adeg_out <- bind_rows(
  adeg |> select(STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU),
  qtcbr,
  qtcfr,
  rrr
) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg_out, "/app/output/adeg.csv", na = "")
