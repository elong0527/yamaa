# Reference solution for the yamaa benchmark adam-adlb-supplb-padded-key (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(LBSEQ = col_integer(), .default = col_character())
)
supp <- read_csv(
  "/app/input/supplb.csv",
  col_types = cols(.default = col_character()),
  trim_ws = FALSE
)

# The qualifier value whose IDVARVAL is the laboratory sequence number as
# eight characters, right-aligned and space-padded. Zero-padded keys do
# not match; a supplemental record with no laboratory record adds no row.
adlb <- lb |>
  mutate(PADDED_KEY = str_pad(as.character(LBSEQ), width = 8, side = "left", pad = " ")) |>
  left_join(
    supp |> select(STUDYID, USUBJID, IDVARVAL, QVAL),
    by = c("STUDYID", "USUBJID", "PADDED_KEY" = "IDVARVAL")
  ) |>
  mutate(QVAL_NAMED = QVAL) |>
  arrange(LBSEQ, USUBJID) |>
  select(STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
