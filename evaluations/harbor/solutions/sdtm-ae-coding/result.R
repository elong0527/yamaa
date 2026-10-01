# Reference solution for the yamaa benchmark sdtm-ae-coding (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae_raw <- read_csv(
  "/app/input/ae_raw.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
meddra <- read_csv(
  "/app/input/meddra_26_1.csv",
  col_types = cols(.default = col_character())
)

# The preferred term and body system of the entry whose lowest-level term
# equals the reported term exactly, including letter case. A blank term or
# one with no exact match is not coded.
ae <- ae_raw |>
  left_join(meddra, by = c("AETERM" = "LLTNAME")) |>
  mutate(
    DOMAIN = "AE",
    AEDECOD = if_else(
      is.na(AETERM) | AETERM == "" | is.na(PTNAME), "NOT CODED", PTNAME
    ),
    AEBODSYS = if_else(
      is.na(AETERM) | AETERM == "" | is.na(SOCNAME), "NOT CODED", SOCNAME
    )
  ) |>
  select(DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, AEBODSYS)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ae, "/app/output/ae.csv", na = "")
