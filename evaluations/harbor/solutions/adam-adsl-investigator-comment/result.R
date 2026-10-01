# Reference solution for the yamaa benchmark adam-adsl-investigator-comment (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character()),
  trim_ws = FALSE
)

# An empty comment field, quoted or not, counts as no comment collected,
# while a field holding only spaces is still a comment and the spaces
# are kept.
adsl <- dm |>
  mutate(
    CMNT = na_if(COMMENT, ""),
    CMNTFL = if_else(is.na(CMNT), "N", "Y")
  ) |>
  select(STUDYID, USUBJID, CMNT, CMNTFL) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
