# Reference solution for the yamaa benchmark adam-adae-query-flags (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
queries <- read_csv(
  "/app/input/queries.csv",
  col_types = cols(GRPID = col_integer(), .default = col_character())
)

# Each standardized slot shows its grouping name, dictionary code, and
# scope together; the sponsor slot shows the grouping name only. Two
# events with the same coded term always show the same entries.
smq01 <- queries |>
  filter(PREFIX %in% "SMQ01") |>
  select(TERM, SMQ01NAM = GRPNAME, SMQ01CD = GRPID, SMQ01SC = SCOPE)

smq02 <- queries |>
  filter(PREFIX %in% "SMQ02") |>
  select(TERM, SMQ02NAM = GRPNAME, SMQ02CD = GRPID, SMQ02SC = SCOPE)

cq01 <- queries |>
  filter(PREFIX %in% "CQ01") |>
  select(TERM, CQ01NAM = GRPNAME)

adae <- ae |>
  left_join(smq01, by = c("AEDECOD" = "TERM")) |>
  left_join(smq02, by = c("AEDECOD" = "TERM")) |>
  left_join(cq01, by = c("AEDECOD" = "TERM")) |>
  select(
    STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, SMQ01NAM, SMQ01CD, SMQ01SC,
    SMQ02NAM, SMQ02CD, SMQ02SC, CQ01NAM
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
