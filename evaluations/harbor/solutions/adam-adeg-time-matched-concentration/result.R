# Reference solution for the yamaa benchmark adam-adeg-time-matched-concentration (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

numbers <- cols(
  APERIOD = col_integer(), APERDAY = col_integer(),
  ATPTN = col_double(), AVAL = col_double(),
  BASE = col_double(), CHG = col_double(),
  .default = col_character()
)
adeg <- read_csv("/app/input/adeg_raw.csv", col_types = numbers, na = "")
adpc <- read_csv(
  "/app/input/adpc.csv",
  col_types = cols(
    APERIOD = col_integer(), APERDAY = col_integer(),
    ATPTN = col_double(), AVAL = col_double(),
    .default = col_character()
  ),
  na = ""
)

# The parent drug's sample at exactly the same subject, period, period day
# and nominal time point; metabolite records are never used.
parent <- adpc |>
  filter(PARAMCD == "DRGX") |>
  transmute(STUDYID, USUBJID, APERIOD, APERDAY, ATPTN,
            SAMPLED = TRUE, PKAVAL = AVAL)

adeg_out <- adeg |>
  left_join(
    parent,
    by = c("STUDYID", "USUBJID", "APERIOD", "APERDAY", "ATPTN"),
    relationship = "many-to-one"
  ) |>
  mutate(
    # A sample whatever its result, also below the limit.
    ECGPCFL = if_else(!is.na(SAMPLED), "Y", NA_character_),
    # As recorded; nothing imputed, except 0 for placebo without a
    # quantified result.
    PKCONC = case_when(
      !is.na(PKAVAL) ~ PKAVAL,
      TRTA == "Placebo" ~ 0,
      .default = NA_real_
    ),
    CQTANLFL = if_else(!is.na(CHG) & !is.na(PKCONC), "Y", NA_character_)
  ) |>
  select(all_of(c(names(adeg), "ECGPCFL", "PKCONC", "CQTANLFL")))

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg_out, "/app/output/adeg.csv", na = "")
