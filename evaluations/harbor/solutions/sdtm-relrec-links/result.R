# Reference solution for the yamaa benchmark sdtm-relrec-links (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(.default = col_character())
)
cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(.default = col_character())
)

ae_numbers <- ae |>
  transmute(
    STUDYID, USUBJID,
    AENO = trimws(AESPID),
    AESEQ = as.integer(AESEQ)
  )

# Each event number on a medication record names the same subject's AE
# record with that AESPID; every number must name one.
pairs <- cm |>
  pivot_longer(
    cols = c(CMAENO1, CMAENO2),
    values_to = "AENO"
  ) |>
  mutate(AENO = trimws(AENO)) |>
  filter(!is.na(AENO) & AENO != "") |>
  inner_join(
    ae_numbers,
    by = c("STUDYID", "USUBJID", "AENO"),
    unmatched = c("error", "drop"),
    relationship = "many-to-one"
  ) |>
  mutate(CMSEQ = as.integer(CMSEQ)) |>
  arrange(STUDYID, USUBJID, AESEQ, CMSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(RELID = as.character(row_number())) |>
  ungroup()

ae_rows <- pairs |>
  transmute(
    STUDYID, USUBJID,
    RDOMAIN = "AE", IDVAR = "AESEQ", IDVARVAL = as.character(AESEQ),
    RELTYPE = NA_character_, RELID
  )
cm_rows <- pairs |>
  transmute(
    STUDYID, USUBJID,
    RDOMAIN = "CM", IDVAR = "CMSEQ", IDVARVAL = as.character(CMSEQ),
    RELTYPE = NA_character_, RELID
  )

relrec <- bind_rows(ae_rows, cm_rows) |>
  arrange(USUBJID, as.integer(RELID), RDOMAIN) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(relrec, "/app/output/relrec.csv", na = "")
