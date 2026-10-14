# Reference solution for the yamaa benchmark adam-adeg-placebo-corrected-change (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adeg <- read_csv(
  "/app/input/adeg_raw.csv",
  col_types = cols(
    APERIOD = col_integer(), APERDAY = col_integer(),
    ATPTN = col_double(), AVAL = col_double(),
    BASE = col_double(), CHG = col_double(),
    .default = col_character()
  ),
  na = ""
)

adeg <- adeg |>
  mutate(
    after_baseline = is.na(ABLFL) | ABLFL != "Y",
    ACOMPFL = if_else(after_baseline & TRTA == "Placebo", "Y", NA_character_)
  )

# The placebo group: every comparator record of the same parameter and
# nominal time point, all cohorts together. Each mean skips missing values;
# a mean over no value has none.
mean_or_na <- function(x) if (all(is.na(x))) NA_real_ else mean(x, na.rm = TRUE)

placebo <- adeg |>
  filter(ACOMPFL %in% "Y") |>
  group_by(PARAMCD, ATPTN) |>
  summarise(
    COMP = mean_or_na(AVAL),
    COMPBASE = mean_or_na(BASE),
    COMPCHG = mean_or_na(CHG),
    .groups = "drop"
  )

adeg_out <- adeg |>
  left_join(placebo, by = c("PARAMCD", "ATPTN"),
            relationship = "many-to-one") |>
  mutate(
    # Baseline records carry no comparator; placebo records carry the
    # group's means like every other record after baseline.
    COMP = if_else(after_baseline, COMP, NA_real_),
    COMPBASE = if_else(after_baseline, COMPBASE, NA_real_),
    COMPCHG = if_else(after_baseline, COMPCHG, NA_real_),
    # The double delta, on drug records only.
    CCOMPCHG = if_else(after_baseline & is.na(ACOMPFL), CHG - COMPCHG,
                       NA_real_),
    COMPTYPE = if_else(after_baseline, paste0("Placebo group at ", ATPT),
                       NA_character_)
  ) |>
  select(
    STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT,
    ATPTN, AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL, COMP, COMPBASE,
    COMPCHG, CCOMPCHG, COMPTYPE
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg_out, "/app/output/adeg.csv", na = "")
