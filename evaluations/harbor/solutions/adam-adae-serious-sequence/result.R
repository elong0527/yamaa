# Reference solution for the yamaa benchmark adam-adae-serious-sequence (R track).
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

# Number the subject's serious events in onset order, so the earliest
# carries 1. Events sharing one onset date follow collection order, and
# a serious event with no onset date is numbered last.
numbered <- ae |>
  filter(AESER %in% "Y") |>
  mutate(ASTDT = if_else(is.na(AESTDTC) | AESTDTC == "", NA_character_, AESTDTC)) |>
  arrange(USUBJID, is.na(ASTDT), ASTDT, AESEQ) |>
  group_by(USUBJID) |>
  mutate(SERSEQ = row_number()) |>
  ungroup() |>
  select(USUBJID, AESEQ, SERSEQ)

adae <- ae |>
  mutate(ASTDT = if_else(is.na(AESTDTC) | AESTDTC == "", NA_character_, AESTDTC)) |>
  left_join(numbered, by = c("USUBJID", "AESEQ")) |>
  select(STUDYID, USUBJID, AESEQ, AESER, ASTDT, SERSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
