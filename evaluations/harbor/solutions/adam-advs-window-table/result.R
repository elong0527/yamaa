# Reference solution for the yamaa benchmark adam-advs-window-table (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr)
library(readr)

raw <- read_csv(
  "/app/input/advs_raw.csv",
  col_types = cols(
    VSSEQ = col_integer(), ADT = col_date(), ADY = col_integer(),
    AVAL = col_double(), .default = col_character()
  )
)
windows <- read_csv(
  "/app/input/awindow.csv",
  col_types = cols(
    AVISITN = col_integer(), AWLO = col_integer(), AWHI = col_integer(),
    AWTARGET = col_integer(), .default = col_character()
  )
)

# The one window in the same study whose first-to-last range, both ends
# inclusive, holds the study day. An absent bound never matches.
match_window <- function(studyid, ady) {
  if (is.na(ady)) {
    return(tibble::tibble(AVISIT = NA_character_, AVISITN = NA_integer_, AWTARGET = NA_integer_))
  }
  hit <- windows |>
    filter(
      STUDYID == studyid, !is.na(AWLO), !is.na(AWHI),
      AWLO <= ady, ady <= AWHI
    ) |>
    slice_head(n = 1)
  if (nrow(hit) == 0) {
    tibble::tibble(AVISIT = NA_character_, AVISITN = NA_integer_, AWTARGET = NA_integer_)
  } else {
    hit |> select(AVISIT, AVISITN, AWTARGET)
  }
}

framed <- raw |>
  arrange(STUDYID, USUBJID, VSSEQ) |>
  mutate(window = map2(STUDYID, ADY, match_window)) |>
  tidyr::unnest(window) |>
  mutate(
    AWTDIFF = ADY - AWTARGET,
    AWTABS = abs(AWTDIFF)
  )

# The record nearest its target in each study, subject, parameter, and
# visit; the lower sequence number breaks a tie.
flagged <- framed |>
  filter(!is.na(AVISIT)) |>
  arrange(AWTABS, VSSEQ) |>
  distinct(STUDYID, USUBJID, PARAMCD, AVISIT, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, PARAMCD, AVISIT, VSSEQ) |>
  mutate(ANL01FL = "Y")

advs <- framed |>
  select(-AWTABS) |>
  left_join(flagged, by = c("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ")) |>
  arrange(STUDYID, USUBJID, VSSEQ) |>
  select(
    STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, ADT, ADY, AVAL,
    AVISIT, AVISITN, AWTARGET, AWTDIFF, ANL01FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
