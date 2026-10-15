# Reference solution for the yamaa benchmark adam-adeg-replicate-qtc-average (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

eg <- read_csv(
  "/app/input/eg.csv",
  col_types = cols(
    EGSTRESN = col_double(),
    EGTPTNUM = col_double(),
    .default = col_character()
  )
)

qt <- eg |> filter(EGTESTCD == "QTAG")
rr <- eg |> filter(EGTESTCD == "RRAG")

# One ECG is one replicate number at a time point: its QT pairs with the RR
# of the same replicate number, never by collection time.
pairs <- qt |>
  select(STUDYID, USUBJID, EGTPTNUM, EGTPT, EGREPNUM, QT = EGSTRESN) |>
  left_join(
    rr |> select(STUDYID, USUBJID, EGTPTNUM, EGREPNUM, RR = EGSTRESN),
    by = c("STUDYID", "USUBJID", "EGTPTNUM", "EGREPNUM")
  ) |>
  # Fridericia's correction of each ECG: QT over the cube root of RR in
  # seconds; zero RR cannot supply a corrected value.
  mutate(QTCF = QT / (na_if(RR, 0) / 1000)^(1 / 3))

# The mean over the ECGs that have a value; no value, no record.
averages <- function(data, value, paramcd, param) {
  data |>
    filter(!is.na(.data[[value]])) |>
    group_by(STUDYID, USUBJID, EGTPTNUM, EGTPT) |>
    summarise(AVAL = mean(.data[[value]]), .groups = "drop") |>
    mutate(PARAMCD = paramcd, PARAM = param)
}

adeg <- bind_rows(
  averages(qt, "EGSTRESN", "QTAG", "QT Interval, Aggregate (msec)"),
  averages(rr, "EGSTRESN", "RRAG", "RR Interval, Aggregate (msec)"),
  averages(pairs, "QTCF", "QTCF", "QTcF Interval, Aggregate (msec)")
) |>
  mutate(ATPT = EGTPT, ATPTN = EGTPTNUM, DTYPE = "AVERAGE") |>
  arrange(STUDYID, USUBJID, PARAMCD, ATPTN) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg, "/app/output/adeg.csv", na = "")
