# Reference solution for the yamaa benchmark adam-adpp-period-treatment (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

pp <- read_csv(
  "/app/input/pp.csv",
  col_types = cols(PPSTRESN = col_double(), .default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXDOSE = col_double(), .default = col_character())
)

# The subject's dose that starts exactly at the reference date and time;
# TREATMENT 1 is period 1, TREATMENT 2 period 2.
doses <- ex |>
  transmute(
    USUBJID,
    PPRFTDTC = EXSTDTC,
    APERIOD = as.integer(sub("^TREATMENT ", "", EPOCH)),
    TRTA = EXTRT,
    DOSEA = EXDOSE,
    DOSEU = EXDOSU
  )

adpp <- pp |>
  left_join(doses, by = c("USUBJID", "PPRFTDTC")) |>
  transmute(
    STUDYID, USUBJID,
    PARCAT1 = PPCAT, PARAMCD = PPTESTCD, PARAM = PPTEST, PPRFTDTC,
    APERIOD, TRTA, DOSEA, DOSEU,
    AVAL = PPSTRESN, AVALU = PPSTRESU
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
