# Reference solution for the yamaa benchmark adam-adae-severity-rank (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

adae <- ae |>
  mutate(
    ASEV = AESEV,
    ASEVN = case_when(
      ASEV == "MILD" ~ 1L,
      ASEV == "MODERATE" ~ 2L,
      ASEV == "SEVERE" ~ 3L
    )
  ) |>
  group_by(USUBJID) |>
  mutate(
    # Competition rank, worst first; unreported sorts after every reported
    # one and shares one rank. Rank is one plus the count of worse events.
    SEVRANK = map_int(ASEVN, function(v) {
      if (is.na(v)) sum(!is.na(ASEVN)) + 1L else sum(ASEVN > v, na.rm = TRUE) + 1L
    }),
    # Dense rank of the distinct severities, worst first, with unreported
    # as one level after every reported one.
    SEVLVL = map_int(ASEVN, function(v) {
      reported <- unique(ASEVN[!is.na(ASEVN)])
      if (is.na(v)) length(reported) + 1L else sum(reported > v) + 1L
    })
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
