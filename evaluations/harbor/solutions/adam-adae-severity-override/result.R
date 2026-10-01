# Reference solution for the yamaa benchmark adam-adae-severity-override (R track).
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
    ASEV = toupper(AESEV),
    # An approved correction reassigns this one event to SEVERE.
    ASEV = if_else(USUBJID == "CATH-01-001" & AESEQ == 2L, "SEVERE", ASEV),
    ASEVN = case_when(
      ASEV == "MILD" ~ 1L,
      ASEV == "MODERATE" ~ 2L,
      ASEV == "SEVERE" ~ 3L,
      ASEV == "LIFE-THREATENING" ~ 4L
    )
  ) |>
  select(STUDYID, USUBJID, AESEQ, ASEV, ASEVN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
