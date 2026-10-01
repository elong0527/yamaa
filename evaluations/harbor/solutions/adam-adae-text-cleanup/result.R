# Reference solution for the yamaa benchmark adam-adae-text-cleanup (R track).
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

adae <- ae |>
  mutate(
    # AE, a hyphen, and exactly three digits; missing is 0, any other shape
    # is -1, and the digits give the number.
    AEREFNUM = case_when(
      is.na(AESPID) | AESPID == "" ~ 0L,
      str_detect(AESPID, "^AE-[0-9]{3}$") ~ suppressWarnings(as.integer(str_sub(AESPID, 4, 6))),
      .default = -1L
    ),
    AETERMLO = tolower(AETERM),
    AERELLC = if_else(is.na(AEREL) | AEREL == "", "not reported", tolower(AEREL)),
    AREL = toupper(AERELLC)
  ) |>
  select(
    STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC, AREL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
