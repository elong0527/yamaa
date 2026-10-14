# Reference solution for the yamaa benchmark adam-adpp-dose-normalized (R track).
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

# The three exposure parameters that get a dose-normalized record.
normalized <- tibble(
  PPTESTCD = c("CMAX", "AUCLST", "AUCIFO"),
  NEWCD = c("CMAXD", "AUCLSTD", "AUCIFOD"),
  NEWPARAM = c(
    "Max Conc Norm by Dose",
    "AUC to Last Nonzero Conc Norm by Dose",
    "AUC Infinity Obs Norm by Dose"
  )
)
mg_per_unit <- tibble(EXDOSU = c("mg", "ug"), MGDIV = c(1, 1000))

# Every PP record as collected.
kept <- pp |>
  transmute(
    STUDYID, USUBJID, VISIT,
    PARAMCD = PPTESTCD,
    PARAM = PPTEST,
    AVAL = PPSTRESN,
    AVALU = PPSTRESU
  )

# The one administration that starts at each profile's reference time, its
# dose in mg (a dose in ug is a thousandth of a mg).
dose <- ex |>
  left_join(mg_per_unit, by = "EXDOSU") |>
  transmute(STUDYID, USUBJID, PPRFTDTC = EXSTDTC, DOSEMG = EXDOSE / MGDIV)

added <- pp |>
  inner_join(normalized, by = "PPTESTCD") |>
  filter(!is.na(PPSTRESN)) |>
  inner_join(dose, by = c("STUDYID", "USUBJID", "PPRFTDTC")) |>
  filter(!is.na(DOSEMG), DOSEMG != 0) |>
  transmute(
    STUDYID, USUBJID, VISIT,
    PARAMCD = NEWCD,
    PARAM = NEWPARAM,
    AVAL = PPSTRESN / DOSEMG,
    AVALU = paste0(PPSTRESU, "/mg")
  )

adpp <- bind_rows(kept, added) |>
  select(STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
