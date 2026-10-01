# Reference solution for the yamaa benchmark sdtm-suppmh-linkage (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

mh <- read_csv(
  "/app/input/mh.csv",
  col_types = cols(.default = col_character())
)
raw <- read_csv(
  "/app/input/mh_supp_raw.csv",
  col_types = cols(.default = col_character())
)

# The parent sequence number, matched on subject and condition term.
parents <- mh |> select(STUDYID, USUBJID, MHSEQ, MHTERM)

suppmh <- raw |>
  pivot_longer(
    cols = c(MHFAMHX, MHCONF),
    names_to = "QNAM",
    values_to = "QVAL"
  ) |>
  filter(!is.na(QVAL) & QVAL != "") |>
  inner_join(parents, by = c("STUDYID", "USUBJID", "MHTERM")) |>
  mutate(
    RDOMAIN = "MH",
    IDVAR = "MHSEQ",
    IDVARVAL = MHSEQ,
    QLABEL = case_when(
      QNAM == "MHFAMHX" ~ "Family History",
      QNAM == "MHCONF" ~ "Confirmed by Medical Records"
    ),
    QORIG = "Collected",
    QEVAL = NA_character_
  ) |>
  arrange(USUBJID, suppressWarnings(as.integer(MHSEQ)), QNAM) |>
  select(
    STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(suppmh, "/app/output/suppmh.csv", na = "")
