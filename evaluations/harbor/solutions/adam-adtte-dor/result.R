# Reference solution for the yamaa benchmark adam-adtte-dor (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(RESPDT = col_date(), NACTDT = col_date(), .default = col_character())
)
adrs <- read_csv(
  "/app/input/adrs_raw.csv",
  col_types = cols(ASEQ = col_integer(), ADT = col_date(), .default = col_character())
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(DSSEQ = col_integer(), DSSTDTC = col_date(), .default = col_character())
)

# Earliest progression and death, lowest sequence number on a tie.
progression <- adrs |>
  filter(AVALC %in% "PD", !is.na(ADT)) |>
  arrange(USUBJID, ADT, ASEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, PDDT = ADT, PDSEQ = ASEQ)

death <- ds |>
  filter(DSDECOD %in% "DEATH", !is.na(DSSTDTC)) |>
  arrange(USUBJID, DSSTDTC, DSSEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, DTHDT = DSSTDTC, DTHSEQ = DSSEQ)

# Last evaluable assessment (a date and a response other than NE), highest
# sequence number on a tie.
last_assessment <- adrs |>
  filter(!is.na(ADT), !is.na(AVALC), AVALC != "NE") |>
  arrange(USUBJID, desc(ADT), desc(ASEQ)) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, LASTDT = ADT, LASTSEQ = ASEQ)

# Where ADT comes from, for each outcome.
sources <- tribble(
  ~OUTCOME,                           ~SRCDOM, ~SRCVAR,
  "DISEASE PROGRESSION",              "ADRS",  "ADT",
  "DEATH",                            "DS",    "DSSTDTC",
  "LAST TUMOUR ASSESSMENT",           "ADRS",  "ADT",
  "START OF NEW ANTI-CANCER THERAPY", "ADSL",  "NACTDT"
)

adtte <- adsl |>
  filter(!is.na(RESPDT)) |>
  left_join(progression, by = "USUBJID") |>
  left_join(death, by = "USUBJID") |>
  left_join(last_assessment, by = "USUBJID") |>
  mutate(
    # The first event; progression wins a same-day tie with death.
    EVENT = case_when(
      !is.na(PDDT) & (is.na(DTHDT) | PDDT <= DTHDT) ~ "DISEASE PROGRESSION",
      !is.na(DTHDT) ~ "DEATH"
    ),
    EVENTDT = if_else(EVENT == "DEATH", DTHDT, PDDT),
    # An event after the start of new therapy does not count.
    EVENT = if_else(!is.na(NACTDT) & EVENTDT > NACTDT, NA_character_, EVENT),
    # Without a counted event, the record is censored at the earlier of the
    # last evaluable assessment and the start of new therapy.
    OUTCOME = case_when(
      !is.na(EVENT) ~ EVENT,
      !is.na(LASTDT) & (is.na(NACTDT) | LASTDT <= NACTDT) ~ "LAST TUMOUR ASSESSMENT",
      !is.na(NACTDT) ~ "START OF NEW ANTI-CANCER THERAPY"
    ),
    ADT = case_when(
      OUTCOME == "DISEASE PROGRESSION" ~ PDDT,
      OUTCOME == "DEATH" ~ DTHDT,
      OUTCOME == "LAST TUMOUR ASSESSMENT" ~ LASTDT,
      OUTCOME == "START OF NEW ANTI-CANCER THERAPY" ~ NACTDT
    ),
    SRCSEQ = case_when(
      OUTCOME == "DISEASE PROGRESSION" ~ PDSEQ,
      OUTCOME == "DEATH" ~ DTHSEQ,
      OUTCOME == "LAST TUMOUR ASSESSMENT" ~ LASTSEQ
    ),
    PARAMCD = "DOR",
    PARAM = "Duration of Response",
    STARTDT = RESPDT,
    AVAL = as.integer(ADT - STARTDT) + 1L,
    CNSR = if_else(is.na(EVENT), 1L, 0L),
    EVNTDESC = coalesce(EVENT, "CENSORED"),
    CNSDTDSC = if_else(is.na(EVENT), OUTCOME, NA_character_)
  ) |>
  left_join(sources, by = "OUTCOME") |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
    CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtte, "/app/output/adtte.csv", na = "")
