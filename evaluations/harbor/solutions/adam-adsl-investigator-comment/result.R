# Reference solution for the yamaa benchmark adam-adsl-investigator-comment (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character()),
  trim_ws = FALSE
)

# A comment field that is empty, quoted or not, or holds only spaces
# counts as no comment collected; any other comment is kept as recorded.
adsl <- dm |>
  mutate(
    CMNT = if_else(grepl("\\S", COMMENT), COMMENT, NA_character_),
    CMNTFL = if_else(is.na(CMNT), "N", "Y")
  ) |>
  select(STUDYID, USUBJID, CMNT, CMNTFL) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
