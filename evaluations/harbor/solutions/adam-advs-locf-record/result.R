# Reference solution for the yamaa benchmark adam-advs-locf-record (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr)
library(readr)
library(tidyr)

plan <- read_csv(
  "/app/input/plan.csv",
  col_types = cols(AVISITN = col_integer(), .default = col_character())
)
vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    AVISITN = col_integer(), AVAL = col_double(), ADT = col_date(),
    QSSEQ = col_integer(), .default = col_character()
  )
)

# Only results selected for analysis can be carried.
candidates <- vs |>
  filter(ANL01FL %in% "Y", !is.na(AVAL))

# The latest non-missing selected result at or before the planned visit:
# the highest visit number, then the highest sequence number.
pick_donor <- function(usubjid, paramcd, avisitn) {
  donor <- candidates |>
    filter(USUBJID == usubjid, PARAMCD == paramcd, AVISITN <= avisitn) |>
    arrange(AVISITN, QSSEQ) |>
    slice_tail(n = 1)
  if (nrow(donor) == 0) {
    tibble::tibble(AVAL = NA_real_, ADT = as.Date(NA), QSSEQ = NA_integer_)
  } else {
    donor |> select(AVAL, ADT, QSSEQ)
  }
}

advs <- plan |>
  arrange(USUBJID, PARAMCD, AVISITN) |>
  mutate(donor = pmap(list(USUBJID, PARAMCD, AVISITN), pick_donor)) |>
  unnest(donor) |>
  select(USUBJID, PARAMCD, AVISITN, AVAL, ADT, QSSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
