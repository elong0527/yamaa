# Reference solution for the yamaa benchmark adam-adae-review-order (R track).
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

adae <- ae |>
  mutate(
    ASEQ = AESEQ,
    ASTDT = if_else(is.na(AESTDTC) | AESTDTC == "", NA_character_, AESTDTC),
    ASEV = if_else(is.na(AESEV) | AESEV == "", NA_character_, AESEV)
  ) |>
  select(STUDYID, USUBJID, ASEQ, AETERM, ASTDT, ASEV)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
