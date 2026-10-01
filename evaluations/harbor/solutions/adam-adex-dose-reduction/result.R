# Reference solution for the yamaa benchmark adam-adex-dose-reduction (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)

adex <- ex |>
  mutate(
    EXSTDTM = if_else(
      !is.na(EXSTDTM) & nchar(EXSTDTM) == 16L, paste0(EXSTDTM, ":00"), EXSTDTM
    ),
    EXDOSE = suppressWarnings(as.numeric(EXDOSE))
  ) |>
  # Chronological treatment-start order within each subject, breaking
  # timestamp ties by sequence number.
  arrange(USUBJID, EXSTDTM, EXSEQ) |>
  group_by(USUBJID) |>
  mutate(
    PREVDOSE = lag(EXDOSE),
    # Y when the current dose is lower than the previous one; no value for
    # the first record and whenever either dose is zero or missing. The
    # previous dose is always the administration just before.
    DOSREDFL = if_else(
      !is.na(EXDOSE) & EXDOSE != 0 & !is.na(PREVDOSE) & PREVDOSE != 0 &
        EXDOSE < PREVDOSE,
      "Y",
      NA_character_
    )
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, EXSEQ, EXSTDTM, EXDOSE, DOSREDFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adex, "/app/output/adex.csv", na = "")
