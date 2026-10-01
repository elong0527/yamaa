# Reference solution for the yamaa benchmark sdtm-lb-ranges (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb_raw <- read_csv(
  "/app/input/lb_raw.csv",
  col_types = cols(LBSEQ = col_integer(), .default = col_character())
)
dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
lbrange <- read_csv(
  "/app/input/lbrange.csv",
  col_types = cols(.default = col_character())
)

lb <- lb_raw |>
  left_join(dm |> select(USUBJID, SEX), by = "USUBJID") |>
  left_join(lbrange, by = c("LBTESTCD", "SEX")) |>
  mutate(
    DOMAIN = "LB",
    LBSTRESU = UNIT,
    LBSTNRLO = NRLO,
    LBSTNRHI = NRHI,
    LBSTRESN = as.character(LBSTRESN),
    LBNRIND = case_when(
      is.na(LBSTRESN) | LBSTRESN == "" ~ NA_character_,
      is.na(NRLO) | NRLO == "" ~ NA_character_,
      is.na(NRHI) | NRHI == "" ~ NA_character_,
      suppressWarnings(as.numeric(LBSTRESN)) <
        suppressWarnings(as.numeric(NRLO)) ~ "LOW",
      suppressWarnings(as.numeric(LBSTRESN)) >
        suppressWarnings(as.numeric(NRHI)) ~ "HIGH",
      TRUE ~ "NORMAL"
    ),
    LBSTRESN = na_if(LBSTRESN, "")
  ) |>
  arrange(USUBJID, LBSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU,
    LBSTNRLO, LBSTNRHI, LBNRIND
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
