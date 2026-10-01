# Reference solution for the yamaa benchmark sdtm-relrec-dataset-level (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(.default = col_character())
)
cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(.default = col_character())
)

studyid <- ae$STUDYID[[1]]

# Whole domains rather than subject records: one tumor identification
# relates to many tumor results.
dataset_rows <- tibble(
  STUDYID = studyid,
  USUBJID = NA_character_,
  RDOMAIN = c("TU", "TR"),
  IDVAR = c("TULNKID", "TRLNKID"),
  IDVARVAL = NA_character_,
  RELTYPE = c("ONE", "MANY"),
  RELID = c("1", "1")
)

ae_rows <- ae |>
  filter(!is.na(AELNKID) & trimws(AELNKID) != "") |>
  mutate(
    RDOMAIN = "AE",
    IDVAR = "AESEQ",
    IDVARVAL = trimws(AESEQ),
    RELTYPE = NA_character_,
    RELID = trimws(AELNKID)
  ) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

cm_rows <- cm |>
  filter(!is.na(CMLNKID) & trimws(CMLNKID) != "") |>
  mutate(
    RDOMAIN = "CM",
    IDVAR = "CMSEQ",
    IDVARVAL = trimws(CMSEQ),
    RELTYPE = NA_character_,
    RELID = trimws(CMLNKID)
  ) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

relrec <- bind_rows(dataset_rows, ae_rows, cm_rows) |>
  select(STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(relrec, "/app/output/relrec.csv", na = "")
