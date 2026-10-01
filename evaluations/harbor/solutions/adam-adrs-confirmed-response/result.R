# Reference solution for the yamaa benchmark adam-adrs-confirmed-response (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

rs <- read_csv(
  "/app/input/rs.csv",
  col_types = cols(
    RSSEQ = col_integer(),
    RSDTC = col_date(),
    .default = col_character()
  )
)

# Each assessment is compared with the next one in analysis date order;
# assessments sharing a date are ordered by sequence number, so the
# higher-numbered one is zero days later and never confirms the lower.
ordered <- rs |>
  mutate(PARAMCD = "CONFRESP", ADT = RSDTC, AVALC = RSSTRESC) |>
  arrange(STUDYID, USUBJID, ADT, RSSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(
    NEXT_AVALC = lead(AVALC),
    NEXT_ADT = lead(ADT),
    DAYS = as.integer(NEXT_ADT - ADT),
    CONFIRMED = case_when(
      AVALC %in% "PD" ~ "Y",
      AVALC %in% c("PR", "CR") &
        NEXT_AVALC %in% c("PR", "CR") &
        !is.na(DAYS) & DAYS >= 28L ~ "Y",
      .default = "N"
    )
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, PARAMCD, RSSEQ, ADT, AVALC, CONFIRMED) |>
  arrange(USUBJID, ADT, RSSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ordered, "/app/output/adrs.csv", na = "")
