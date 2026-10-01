# Reference solution for the yamaa benchmark adam-adae-severity (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
supp <- read_csv(
  "/app/input/supp.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
) |>
  select(STUDYID, USUBJID, AESEQ, AESEV)

# Severity is matched on the study and subject identifiers together with
# the event sequence number. An event with no supplemental record, or a
# record carrying no severity, has no value.
adae <- ae |>
  left_join(supp, by = c("STUDYID", "USUBJID", "AESEQ")) |>
  mutate(AESEV = if_else(is.na(AESEV) | AESEV == "", NA_character_, AESEV)) |>
  select(STUDYID, USUBJID, AESEQ, AEDECOD, AESEV)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
