# Reference solution for the yamaa benchmark sdtm-lb-compartments (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(purrr)

bx <- read_csv(
  "/app/input/bx_raw.csv",
  col_types = cols(.default = col_character())
)

make_records <- function(study, subj, cohort, les, nles, unit) {
  locs <- list()
  if (!is.na(cohort) && cohort != "" && cohort != "NONAD") {
    locs <- append(locs, list(list(loc = "LESIONAL", res = les)))
  }
  locs <- append(locs, list(list(loc = "NON-LESIONAL", res = nles)))
  imap(locs, function(entry, i) {
    res <- entry$res
    if (is.na(res) || res == "") res <- NA_character_
    num <- suppressWarnings(as.numeric(res))
    tibble(
      DOMAIN = "LB",
      STUDYID = study,
      USUBJID = subj,
      LBSEQ = as.integer(i),
      LBTESTCD = "IL13",
      LBTEST = "Interleukin 13",
      LBSPEC = "SKIN",
      LBLOC = entry$loc,
      LBORRES = res,
      LBORRESU = unit,
      LBSTRESN = num,
      LBSTAT = if_else(is.na(res), "NOT DONE", NA_character_)
    )
  }) |> bind_rows()
}

lb <- pmap_dfr(
  list(bx$STUDYID, bx$USUBJID, bx$COHORT, bx$LESRES, bx$NLESRES, bx$RESU),
  make_records
) |>
  arrange(STUDYID, USUBJID, LBSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
