# Reference solution for the yamaa benchmark adam-adae-onset-emergence (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)

# Datetimes collected without seconds are completed to the second so the
# onset and the first exposure compare at the moment, not the day.
complete_seconds <- function(x) {
  if_else(
    !is.na(x) & str_detect(x, "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}$"),
    paste0(x, ":00"),
    x
  )
}

ae <- ae |>
  mutate(ASTDTM = complete_seconds(AESTDTC)) |>
  select(STUDYID, USUBJID, AESEQ, AETERM, ASTDTM)

adsl <- adsl |>
  mutate(TRTSDTM = complete_seconds(TRTSDTM)) |>
  select(USUBJID, TRTSDTM)

adae <- ae |>
  left_join(adsl, by = "USUBJID") |>
  mutate(
    TRTEMFL = if_else(
      !is.na(ASTDTM) & !is.na(TRTSDTM) & ASTDTM >= TRTSDTM,
      "Y",
      NA_character_
    )
  )

# The subject's earliest treatment-emergent event, ordered by onset
# moment with the lower AESEQ settling ties at the same second.
first <- adae |>
  filter(TRTEMFL %in% "Y") |>
  arrange(USUBJID, ASTDTM, AESEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, FIRST_SEQ = AESEQ)

adae <- adae |>
  left_join(first, by = "USUBJID") |>
  mutate(
    AOCCFL = if_else(!is.na(FIRST_SEQ) & AESEQ == FIRST_SEQ, "Y", NA_character_)
  ) |>
  select(STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TRTEMFL, AOCCFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
