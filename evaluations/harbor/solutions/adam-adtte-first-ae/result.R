# Reference solution for the yamaa benchmark adam-adtte-first-ae (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), EOSDT = col_date(), .default = col_character())
)
adae <- read_csv(
  "/app/input/adae.csv",
  col_types = cols(AESEQ = col_integer(), ASTDT = col_date(), .default = col_character())
)

# The subject's first adverse event by onset date, ties to the lower
# sequence; an event with no onset sorts last, so it is passed over when
# a dated event exists.
first_ae <- adae |>
  filter(!is.na(ASTDT)) |>
  arrange(STUDYID, USUBJID, ASTDT, AESEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, AEDT = ASTDT, AESEQ)

adtte <- adsl |>
  left_join(first_ae, by = c("STUDYID", "USUBJID")) |>
  mutate(
    STARTDT = TRTSDT,
    # The earliest onset, or the end of study when no event carries
    # a usable onset date.
    RAWDT = coalesce(AEDT, EOSDT),
    # A date before treatment start is moved up to it, keeping its
    # event-or-censoring status and source; with no treatment start
    # the date is kept as it is.
    ADT = case_when(
      !is.na(STARTDT) & !is.na(RAWDT) & STARTDT > RAWDT ~ STARTDT,
      .default = RAWDT
    ),
    AVAL = as.integer(ADT - STARTDT) + 1L,
    CNSR = if_else(!is.na(AEDT), 0L, 1L),
    EVNTDESC = if_else(!is.na(AEDT), "AE", "END OF STUDY"),
    SRCDOM = if_else(!is.na(AEDT), "ADAE", "ADSL"),
    SRCVAR = if_else(!is.na(AEDT), "ASTDT", "EOSDT"),
    SRCSEQ = if_else(!is.na(AEDT), AESEQ, NA_integer_),
    PARAMCD = "TTAE",
    PARAM = "Time to First Adverse Event"
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR,
    EVNTDESC, SRCDOM, SRCVAR, SRCSEQ
  ) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtte, "/app/output/adtte.csv", na = "")
