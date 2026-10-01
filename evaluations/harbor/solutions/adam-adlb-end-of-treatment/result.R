# Reference solution for the yamaa benchmark adam-adlb-end-of-treatment (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    LBSEQ = col_integer(),
    VISITNUM = col_integer(),
    LBSTRESN = col_double(),
    .default = col_character()
  )
)
supp <- read_csv(
  "/app/input/supplb.csv",
  col_types = cols(.default = col_character())
)

# ENDPOINT is Y when a supplemental endpoint qualifier with the same
# study, subject, and sequence number marks the record. Only the protocol
# endpoint qualifier is read.
endpoint_keys <- supp |>
  filter(QNAM %in% "ENDPOINT") |>
  transmute(
    STUDYID, USUBJID,
    SEQ_TRIM = str_trim(IDVARVAL),
    ENDPOINT = "Y"
  )

marked <- lb |>
  mutate(SEQ_TRIM = as.character(LBSEQ), AVAL = LBSTRESN) |>
  left_join(endpoint_keys, by = c("STUDYID", "USUBJID", "SEQ_TRIM")) |>
  select(-SEQ_TRIM)

# EOTFL is Y on one record per subject and test: the endpoint-marked
# record when there is one, otherwise the record from the latest visit.
endpoint_winners <- marked |>
  filter(ENDPOINT %in% "Y") |>
  group_by(STUDYID, USUBJID, LBTESTCD) |>
  summarise(WIN_ENDPOINT = min(LBSEQ), .groups = "drop")

latest_winners <- marked |>
  arrange(desc(VISITNUM), desc(LBSEQ)) |>
  distinct(STUDYID, USUBJID, LBTESTCD, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, LBTESTCD, WIN_LATEST = LBSEQ)

winners <- latest_winners |>
  left_join(endpoint_winners, by = c("STUDYID", "USUBJID", "LBTESTCD")) |>
  mutate(WIN_LBSEQ = coalesce(WIN_ENDPOINT, WIN_LATEST), EOTFL = "Y") |>
  select(STUDYID, USUBJID, LBTESTCD, WIN_LBSEQ, EOTFL)

adlb <- marked |>
  left_join(
    winners,
    by = c("STUDYID", "USUBJID", "LBTESTCD", "LBSEQ" = "WIN_LBSEQ")
  ) |>
  arrange(USUBJID, LBTESTCD, LBSEQ) |>
  select(STUDYID, USUBJID, LBSEQ, LBTESTCD, VISITNUM, AVAL, ENDPOINT, EOTFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
