# Reference solution for the yamaa benchmark sdtm-lb-last-observation-before-exposure (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(purrr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
) |>
  mutate(Value = na_if(Value, ""))

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
) |>
  mutate(RFSTDTC = na_if(RFSTDTC, ""))

mapping <- read_csv(
  "/app/input/lb_mapping.csv",
  col_types = cols(.default = col_character())
)

groups <- odm |>
  distinct(StudyOID, SubjectKey, ItemGroupRepeatKey)

one_group <- function(study, subj, igrep) {
  sub <- odm |>
    filter(SubjectKey == subj, ItemGroupRepeatKey == igrep)
  get <- function(oid) {
    hit <- sub |> filter(ItemOID == oid)
    if (nrow(hit) == 0) return(NA_character_)
    hit$Value[[1]]
  }
  lbdtc <- get("IT.LB.LBDTC")
  found <- mapping |>
    filter(ItemOID %in% sub$ItemOID) |>
    slice_head(n = 1)
  if (nrow(found) == 0) return(tibble())
  raw <- get(found$ItemOID[[1]])
  has_result <- !is.na(raw)
  tibble(
    DOMAIN = "LB",
    STUDYID = study,
    USUBJID = subj,
    LBSEQ = as.integer(igrep),
    LBTESTCD = found$LBTESTCD[[1]],
    LBSPEC = found$LBSPEC[[1]],
    LBORRES = raw,
    LBDTC = lbdtc,
    LBSTAT = if_else(has_result, NA_character_, "NOT DONE"),
    HAS = has_result
  )
}

lb <- pmap_dfr(
  list(groups$StudyOID, groups$SubjectKey, groups$ItemGroupRepeatKey),
  one_group
)

flags <- lb |>
  filter(HAS, !is.na(LBDTC)) |>
  left_join(dm |> select(USUBJID, RFSTDTC), by = "USUBJID") |>
  filter(is.na(RFSTDTC) | LBDTC <= RFSTDTC) |>
  arrange(USUBJID, LBTESTCD, LBSPEC, LBDTC, LBSEQ) |>
  group_by(USUBJID, LBTESTCD, LBSPEC) |>
  slice_tail(n = 1) |>
  ungroup() |>
  transmute(USUBJID, LBTESTCD, LBSPEC, FLAGSEQ = LBSEQ)

lb <- lb |>
  left_join(flags, by = c("USUBJID", "LBTESTCD", "LBSPEC")) |>
  mutate(
    LBLOBXFL = if_else(HAS & !is.na(FLAGSEQ) & LBSEQ == FLAGSEQ, "Y", NA_character_)
  ) |>
  arrange(STUDYID, USUBJID, LBSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES,
    LBDTC, LBSTAT, LBLOBXFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
