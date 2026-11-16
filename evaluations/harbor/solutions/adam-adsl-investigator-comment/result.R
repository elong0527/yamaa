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
  na = ""
)
co <- read_csv(
  "/app/input/co.csv",
  col_types = cols(COSEQ = col_integer(), .default = col_character()),
  na = "",
  trim_ws = FALSE
)

# Each comment is COVAL, then COVAL1 after one space when it continues
# there; a subject's comments join in COSEQ order with "; ".
comments <- co |>
  mutate(TEXT = if_else(is.na(COVAL1) | COVAL1 == "", COVAL,
                        paste(COVAL, COVAL1))) |>
  arrange(USUBJID, COSEQ) |>
  group_by(USUBJID) |>
  summarise(CMNT = paste(TEXT, collapse = "; "), .groups = "drop")

adsl <- dm |>
  left_join(comments, by = "USUBJID") |>
  mutate(CMNTFL = if_else(is.na(CMNT), "N", "Y")) |>
  select(STUDYID, USUBJID, CMNT, CMNTFL) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
