# Reference solution for the yamaa benchmark sdtm-rs-timepoint-response (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

visits <- odm |>
  filter(ItemGroupOID == "IG.VISIT") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, StudyEventOID),
    names_from = ItemOID,
    values_from = Value,
    values_fn = function(x) x[[1]]
  )

tr_target <- odm |>
  filter(grepl("TARGET", ItemGroupOID, fixed = TRUE),
    startsWith(ItemGroupOID, "IG.TR")) |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, StudyEventOID, ItemGroupRepeatKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = function(x) x[[1]]
  )

tr_any <- odm |>
  filter(startsWith(ItemGroupOID, "IG.TR")) |>
  count(SubjectKey, StudyEventOID, name = "n_tr")

tu_target <- odm |>
  filter(grepl("TARGET", ItemGroupOID, fixed = TRUE),
    startsWith(ItemGroupOID, "IG.TU"),
    StudyEventOID == "BASELINE",
    ItemOID == "IT.TU.TULNKID",
    !is.na(Value) & Value != "") |>
  distinct(SubjectKey, Value)

response_one <- function(subj, event, avisitn, base_sum, n_chosen, post_diams, has_records) {
  if (!has_records) {
    return(list(rsstresc = NA_character_, rsstat = "NOT DONE"))
  }
  if (event == "BASELINE") {
    return(list(rsstresc = "NE", rsstat = NA_character_))
  }
  if (n_chosen == 0) {
    return(list(rsstresc = "NE", rsstat = NA_character_))
  }
  measured <- post_diams[!is.na(post_diams) & post_diams != ""]
  if (length(measured) < n_chosen) {
    return(list(rsstresc = "NE", rsstat = NA_character_))
  }
  if (is.na(base_sum) || base_sum == 0) {
    return(list(rsstresc = "NE", rsstat = NA_character_))
  }
  post <- sum(suppressWarnings(as.numeric(measured)))
  if (all(suppressWarnings(as.numeric(measured)) == 0)) {
    return(list(rsstresc = "CR", rsstat = NA_character_))
  }
  if ((base_sum - post) / base_sum >= 0.30) {
    return(list(rsstresc = "PR", rsstat = NA_character_))
  }
  if ((post - base_sum) / base_sum >= 0.20) {
    return(list(rsstresc = "PD", rsstat = NA_character_))
  }
  list(rsstresc = "SD", rsstat = NA_character_)
}

subjects <- unique(visits$SubjectKey)
out <- list()
for (subj in subjects) {
  chosen <- nrow(tu_target |> filter(SubjectKey == subj))
  if (chosen == 0) {
    chosen <- nrow(tr_target |>
      filter(SubjectKey == subj, StudyEventOID == "BASELINE"))
  }
  base_rows <- tr_target |>
    filter(SubjectKey == subj, StudyEventOID == "BASELINE")
  base_diams <- base_rows$IT.TR.LDIAM
  base_sum <- if (chosen == 0) {
    NA_real_
  } else if (any(is.na(base_diams) | base_diams == "")) {
    NA_real_
  } else {
    sum(suppressWarnings(as.numeric(base_diams)))
  }
  subj_visits <- visits |>
    filter(SubjectKey == subj) |>
    arrange(suppressWarnings(as.integer(IT.VISIT.AVISITN)))
  seq <- 0L
  for (i in seq_len(nrow(subj_visits))) {
    seq <- seq + 1L
    event <- subj_visits$StudyEventOID[[i]]
    n_rec <- tr_any |>
      filter(SubjectKey == subj, StudyEventOID == event) |>
      pull(n_tr)
    has_records <- length(n_rec) > 0 && n_rec[[1]] > 0
    post_diams <- tr_target |>
      filter(SubjectKey == subj, StudyEventOID == event) |>
      pull(IT.TR.LDIAM)
    resp <- response_one(
      subj, event, subj_visits$IT.VISIT.AVISITN[[i]], base_sum, chosen,
      post_diams, has_records
    )
    out[[length(out) + 1]] <- tibble(
      STUDYID = subj_visits$StudyOID[[i]],
      USUBJID = subj,
      AVISIT = subj_visits$IT.VISIT.AVISIT[[i]],
      AVISITN = subj_visits$IT.VISIT.AVISITN[[i]],
      ADT = subj_visits$IT.VISIT.ADT[[i]],
      RSSEQ = seq,
      RSTESTCD = "TRGRESP",
      RSTEST = "Timepoint Response",
      RSSTRESC = resp$rsstresc,
      RSSTAT = resp$rsstat
    )
  }
}

rs <- bind_rows(out) |>
  arrange(USUBJID, suppressWarnings(as.integer(AVISITN))) |>
  select(
    STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST,
    RSSTRESC, RSSTAT
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(rs, "/app/output/rs.csv", na = "")
