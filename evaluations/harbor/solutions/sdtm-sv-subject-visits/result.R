# Reference solution for the yamaa benchmark sdtm-sv-subject-visits (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(readr)
library(tidyr)

study_day <- function(ref, coll) {
  if (is.na(ref) || is.na(coll)) {
    return(NA_integer_)
  }
  delta <- as.integer(coll - ref)
  if (delta >= 0L) delta + 1L else delta
}

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

ref_map <- setNames(suppressWarnings(ymd(dm$RFSTDTC)), dm$USUBJID)

wide <- odm |>
  mutate(visit_key = paste(SubjectKey, StudyEventOID, StudyEventRepeatKey, sep = "|")) |>
  select(visit_key, SubjectKey, StudyOID, StudyEventOID, ItemOID, Value) |>
  pivot_wider(names_from = ItemOID, values_from = Value, values_fn = first)

sv <- wide |>
  rowwise() |>
  mutate(
    dates = list(na.omit(c(IT.VS.VSDTC, IT.LB.LBDTC, IT.EX.EXDTC))),
    SVSTDTC = if (length(dates) == 0) NA_character_ else min(dates),
    SVENDTC = if (length(dates) == 0) NA_character_ else max(dates)
  ) |>
  ungroup() |>
  filter(!is.na(SVSTDTC)) |>
  transmute(
    DOMAIN = "SV",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    VISIT = IT.TV.VISIT,
    VISITNUM = suppressWarnings(as.numeric(IT.TV.VISITNUM)),
    VISITDY = suppressWarnings(as.integer(IT.TV.VISITDY)),
    SVSTDTC,
    SVENDTC,
    SVSTDY = vapply(
      seq_len(n()),
      function(i) study_day(ref_map[[USUBJID[i]]], suppressWarnings(ymd(SVSTDTC[i]))),
      integer(1)
    ),
    SVENDY = vapply(
      seq_len(n()),
      function(i) study_day(ref_map[[USUBJID[i]]], suppressWarnings(ymd(SVENDTC[i]))),
      integer(1)
    ),
    TAETORD = suppressWarnings(as.integer(IT.TV.TAETORD)),
    EPOCH = IT.TV.EPOCH,
    SVUPDES = IT.TV.UPDES
  ) |>
  arrange(USUBJID, SVSTDTC) |>
  group_by(USUBJID) |>
  mutate(SVSEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, SVSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC,
    SVENDTC, SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(sv, "/app/output/sv.csv", na = "")
