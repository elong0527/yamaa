# Reference solution for the yamaa benchmark adam-adpp-carryover-exclusion (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

pp <- read_csv(
  "/app/input/pp.csv",
  col_types = cols(
    PPSTRESN = col_double(),
    VISITNUM = col_integer(),
    .default = col_character()
  )
)
pc <- read_csv(
  "/app/input/pc.csv",
  col_types = cols(PCSTRESN = col_double(), .default = col_character())
)

# The period's pre-dose sample: planned at PREDOSE, with the parameter's
# reference dose time. A below-limit result ("<" and the limit) counts as 0;
# a sample not done has no result and gives no value.
predose <- pc |>
  filter(PCTPT == "PREDOSE") |>
  transmute(
    STUDYID, USUBJID,
    PPRFTDTC = PCRFTDTC,
    PRECONC = if_else(
      !is.na(PCSTRESC) & str_starts(PCSTRESC, fixed("<")), 0, PCSTRESN
    )
  )
stopifnot(!anyDuplicated(predose[c("STUDYID", "USUBJID", "PPRFTDTC")]))

# The same subject's Cmax in the same period.
cmax <- pp |>
  filter(PPTESTCD == "CMAX") |>
  transmute(STUDYID, USUBJID, PPRFTDTC, CMAX = PPSTRESN)
stopifnot(!anyDuplicated(cmax[c("STUDYID", "USUBJID", "PPRFTDTC")]))

keys <- c("STUDYID", "USUBJID", "PPRFTDTC")
adpp <- pp |>
  left_join(predose, by = keys) |>
  left_join(cmax, by = keys) |>
  mutate(
    PREPCMAX = if_else(!is.na(CMAX) & CMAX != 0, 100 * PRECONC / CMAX, NA_real_),
    # Left out only when the percentage is known and above 5.
    ANL01FL = if_else(!is.na(PREPCMAX) & PREPCMAX > 5, NA_character_, "Y")
  ) |>
  transmute(
    STUDYID, USUBJID, VISITNUM, VISIT,
    PARAMCD = PPTESTCD, PARAM = PPTEST, AVAL = PPSTRESN,
    PRECONC, PREPCMAX, ANL01FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
