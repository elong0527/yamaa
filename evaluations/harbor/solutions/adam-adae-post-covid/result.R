# Reference solution for the yamaa benchmark adam-adae-post-covid (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

# Each subject's first COVID-19 event: the earliest start date, with the
# higher sequence number ordered after on the same date.
first_covid <- ae |>
  filter(AEDECOD %in% "COVID-19", !is.na(AESTDTC), AESTDTC != "") |>
  arrange(USUBJID, AESTDTC, AESEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, FIRST_DT = AESTDTC, FIRST_SEQ = AESEQ)

adae <- ae |>
  left_join(first_covid, by = "USUBJID") |>
  mutate(
    ASTDT = if_else(is.na(AESTDTC) | AESTDTC == "", NA_character_, AESTDTC),
    AFTCOVFL = if_else(
      !is.na(ASTDT) & !is.na(FIRST_DT) &
        (ASTDT > FIRST_DT | (ASTDT == FIRST_DT & AESEQ > FIRST_SEQ)),
      "Y",
      NA_character_
    )
  ) |>
  select(STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, AFTCOVFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
