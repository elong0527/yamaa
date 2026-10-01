# Reference solution for the yamaa benchmark adam-adae-death (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), ASTDT = col_date(), .default = col_character())
)
dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(DTHDT = col_date(), .default = col_character())
)

# The fatal adverse event behind each death: the most recent one, and the
# highest AESEQ among those starting that day.
fatal <- ae |>
  filter(AEOUT %in% "FATAL") |>
  arrange(USUBJID, desc(ASTDT), desc(AESEQ)) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, DTHCAUS = AEDECOD, FATALDT = ASTDT)

# One record per subject who died, from a fatal adverse event, DM, or both.
death <- dm |>
  filter(!is.na(DTHDT)) |>
  select(USUBJID, DMDTHDT = DTHDT) |>
  full_join(fatal, by = "USUBJID") |>
  mutate(DTHFL = "Y", DTHDT = coalesce(FATALDT, DMDTHDT)) |>
  select(USUBJID, DTHFL, DTHCAUS, DTHDT)

adae <- ae |>
  left_join(death, by = "USUBJID") |>
  select(STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, DTHFL, DTHCAUS, DTHDT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
