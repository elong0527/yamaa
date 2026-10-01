# Reference solution for the yamaa benchmark adam-adsl-rescue-med (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)

# The first rescue medication by earliest start date, with the smaller
# sequence number breaking a tie. A start holding only year and month
# compares as written, so it falls before any full date in the same
# month; a record with no start date still counts.
first <- cm |>
  filter(CMCAT == "RESCUE MEDICATION") |>
  mutate(sortkey = coalesce(CMSTDTC, "")) |>
  arrange(sortkey, CMSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, RESCTRT = CMTRT)

adsl <- dm |>
  left_join(first, by = c("STUDYID", "USUBJID")) |>
  select(STUDYID, USUBJID, RESCTRT) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
