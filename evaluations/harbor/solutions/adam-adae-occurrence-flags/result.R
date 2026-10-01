# Reference solution for the yamaa benchmark adam-adae-occurrence-flags (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/adae_raw.csv",
  col_types = cols(AESEQ = col_integer(), ASTDT = col_date(), .default = col_character())
)

# The first treatment-emergent event at each level: the earliest start
# date, with the lower AESEQ breaking ties on the same day.
first_overall <- ae |>
  filter(TRTEMFL %in% "Y") |>
  arrange(USUBJID, ASTDT, AESEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, FIRST_SEQ = AESEQ)

first_soc <- ae |>
  filter(TRTEMFL %in% "Y") |>
  arrange(USUBJID, AEBODSYS, ASTDT, AESEQ) |>
  distinct(USUBJID, AEBODSYS, .keep_all = TRUE) |>
  select(USUBJID, AEBODSYS, SOC_SEQ = AESEQ)

first_pt <- ae |>
  filter(TRTEMFL %in% "Y") |>
  arrange(USUBJID, AEBODSYS, AEDECOD, ASTDT, AESEQ) |>
  distinct(USUBJID, AEBODSYS, AEDECOD, .keep_all = TRUE) |>
  select(USUBJID, AEBODSYS, AEDECOD, PT_SEQ = AESEQ)

adae <- ae |>
  left_join(first_overall, by = "USUBJID") |>
  left_join(first_soc, by = c("USUBJID", "AEBODSYS")) |>
  left_join(first_pt, by = c("USUBJID", "AEBODSYS", "AEDECOD")) |>
  mutate(
    AOCCFL = if_else(!is.na(FIRST_SEQ) & AESEQ == FIRST_SEQ, "Y", NA_character_),
    AOCCSFL = if_else(!is.na(SOC_SEQ) & AESEQ == SOC_SEQ, "Y", NA_character_),
    AOCCPFL = if_else(!is.na(PT_SEQ) & AESEQ == PT_SEQ, "Y", NA_character_)
  ) |>
  select(
    STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, TRTEMFL,
    AOCCFL, AOCCSFL, AOCCPFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
