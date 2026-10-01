# Reference solution for the yamaa benchmark adam-adrs-best-response (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(RANDDT = col_date(), .default = col_character())
)
selection <- read_csv(
  "/app/input/adrs_selection.csv",
  col_types = cols(
    ASEQ = col_integer(),
    ADT = col_date(),
    RANDDY = col_integer(),
    BORPRI = col_integer(),
    BORSEQ = col_integer(),
    .default = col_character()
  )
)

# The best overall response is the selection record numbered 1.
best <- selection |>
  filter(BORSEQ == 1L) |>
  select(USUBJID, BORCAT, BESTDT = ADT)

adrs <- adsl |>
  left_join(best, by = "USUBJID") |>
  mutate(
    PARAMCD = "BOR",
    PARAM = "Best Overall Response by Investigator",
    AVALC = BORCAT,
    AVAL = case_when(
      AVALC == "CR" ~ 1L,
      AVALC == "PR" ~ 2L,
      AVALC == "SD" ~ 3L,
      AVALC == "NON-CR/NON-PD" ~ 4L,
      AVALC == "PD" ~ 5L,
      AVALC == "NE" ~ 6L
    ),
    ADT = BESTDT
  ) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adrs, "/app/output/adrs.csv", na = "")
