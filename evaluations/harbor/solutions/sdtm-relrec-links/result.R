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

# A record naming two link identifiers contributes one row per identifier.
ae_rows <- ae |>
  pivot_longer(
    cols = c(AELNKID1, AELNKID2),
    values_to = "RELID"
  ) |>
  filter(!is.na(RELID) & trimws(RELID) != "") |>
  mutate(
    RDOMAIN = "AE",
    IDVAR = "AESEQ",
    IDVARVAL = trimws(AESEQ),
    # Link numbers as text without a decimal point.
    RELID = as.character(suppressWarnings(as.integer(RELID))),
    RELTYPE = NA_character_
  ) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

cm_rows <- cm |>
  pivot_longer(
    cols = c(CMLNKID1, CMLNKID2),
    values_to = "RELID"
  ) |>
  filter(!is.na(RELID) & trimws(RELID) != "") |>
  mutate(
    RDOMAIN = "CM",
    IDVAR = "CMSEQ",
    IDVARVAL = trimws(CMSEQ),
    RELID = as.character(suppressWarnings(as.integer(RELID))),
    RELTYPE = NA_character_
  ) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

relrec <- bind_rows(ae_rows, cm_rows) |>
  arrange(
    USUBJID, suppressWarnings(as.integer(RELID)), RDOMAIN,
    suppressWarnings(as.integer(IDVARVAL))
  ) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(relrec, "/app/output/relrec.csv", na = "")
