# Reference solution for the yamaa benchmark adam-adpp-geometric-mean-ratio (R track).
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
pc <- read_csv(
  "/app/input/pc.csv",
  col_types = cols(PCSTRESN = col_double(), .default = col_character())
)

geomean <- pp |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    AVAL = {
      x <- PPSTRESN[PPTESTCD == "MRCMAX" & !is.na(PPSTRESN) & PPSTRESN > 0]
      if (length(x) > 0) exp(mean(log(x))) else NA_real_
    },
    .groups = "drop"
  ) |>
  mutate(
    PARAMCD = "MRCMAXGM",
    PARAM = "Geometric Mean Metabolite Ratio Cmax",
    AVALU = "RATIO"
  )

replicate_sd <- pc |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    AVAL = {
      x <- PCSTRESN[PCTPT == "2H" & !is.na(PCSTRESN)]
      if (length(x) >= 2) sd(x) else NA_real_
    },
    .groups = "drop"
  ) |>
  mutate(
    PARAMCD = "C2HREPSD",
    PARAM = "Replicate SD of 2 H Concentration",
    AVALU = "ng/mL"
  )

adpp <- bind_rows(geomean, replicate_sd) |>
  arrange(STUDYID, USUBJID, PARAMCD) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
