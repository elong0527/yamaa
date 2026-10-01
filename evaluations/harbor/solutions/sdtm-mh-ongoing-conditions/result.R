# Reference solution for the yamaa benchmark sdtm-mh-ongoing-conditions (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

mh_form <- read_csv(
  "/app/input/mh_form.csv",
  col_types = cols(MHSEQ = col_integer(), .default = col_character())
)

mh <- mh_form |>
  mutate(
    MHSTDTC = case_when(
      is.na(MHSTYY) | MHSTYY == "" ~ NA_character_,
      is.na(MHSTMM) | MHSTMM == "" ~ MHSTYY,
      TRUE ~ paste0(MHSTYY, "-", sprintf("%02d", as.integer(MHSTMM)))
    ),
    # A condition still active at screening carries no end date.
    MHENDTC = if_else(MHONGO == "Y" | is.na(MHENDTC) | MHENDTC == "",
      NA_character_, MHENDTC
    ),
    MHENRTPT = case_when(
      MHONGO == "Y" ~ "ONGOING",
      !is.na(MHENDTC) & MHENDTC != "" &
        !is.na(SCRFDTC) & MHENDTC < SCRFDTC ~ "BEFORE",
      TRUE ~ NA_character_
    ),
    MHENTPT = if_else(is.na(MHENRTPT), NA_character_, "SCREENING")
  ) |>
  arrange(USUBJID, MHSEQ) |>
  select(
    STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(mh, "/app/output/mh.csv", na = "")
