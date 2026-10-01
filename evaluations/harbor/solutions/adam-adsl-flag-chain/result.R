# Reference solution for the yamaa benchmark adam-adsl-flag-chain (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(AGE = col_integer(), RANDDT = col_date(), .default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), EXSTDTC = col_date(), .default = col_character())
)

first_exposure <- ex |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    TRTSDT = if (all(is.na(EXSTDTC))) as.Date(NA) else min(EXSTDTC, na.rm = TRUE),
    .groups = "drop"
  )

adsl <- dm |>
  left_join(first_exposure, by = c("STUDYID", "USUBJID")) |>
  mutate(
    SAFFL = if_else(!is.na(TRTSDT), "Y", "N"),
    RANDFL = if_else(!is.na(RANDDT), "Y", "N"),
    ITTFL = if_else(RANDFL == "Y", "Y", "N"),
    POPFL = if_else(SAFFL == "Y" & ITTFL == "Y", "Y", "N"),
    AGEGR1 = if_else(AGE < 65, "<65", ">=65")
  ) |>
  mutate(.idx = row_number()) |>
  arrange(STUDYID, AGE, USUBJID) |>
  group_by(STUDYID) |>
  mutate(AGERNK = row_number()) |>
  ungroup() |>
  arrange(.idx) |>
  select(
    POPFL, SAFFL, ITTFL, TRTSDT, RANDDT, AGEGR1, AGERNK,
    STUDYID, USUBJID, AGE
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
