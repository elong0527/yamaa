# Reference solution for the yamaa benchmark sdtm-se-subject-elements (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

wide <- odm |>
  mutate(Value = na_if(Value, "")) |>
  pivot_wider(
    id_cols = c(SubjectKey, StudyEventOID),
    names_from = ItemOID,
    values_from = Value,
    values_fn = function(x) x[[1]]
  )

study_day <- function(ref, day) {
  ifelse(
    is.na(ref) | ref == "" | is.na(day) | day == "",
    NA_character_,
    {
      delta <- as.integer(as.Date(day) - as.Date(ref))
      as.character(ifelse(delta >= 0, delta + 1L, delta))
    }
  )
}

se <- wide |>
  left_join(dm, by = c("SubjectKey" = "USUBJID")) |>
  mutate(
    DOMAIN = "SE",
    STUDYID = STUDYID,
    USUBJID = SubjectKey,
    SESEQ = IT.TA.TAETORD,
    ETCD = StudyEventOID,
    ELEMENT = IT.TE.ELEMENT,
    TAETORD = IT.TA.TAETORD,
    EPOCH = IT.TA.EPOCH,
    first_dtc = IT.EX.FIRSTDTC,
    last_dtc = IT.EX.LASTDTC,
    SESTDTC = case_when(
      EPOCH == "SCREENING" ~ RFICDTC,
      EPOCH == "TREATMENT" ~ first_dtc,
      TRUE ~ last_dtc
    ),
    SEENDTC = case_when(
      EPOCH == "SCREENING" ~ first_dtc,
      EPOCH == "TREATMENT" ~ last_dtc,
      TRUE ~ RFPENDTC
    ),
    SESTDY = study_day(RFSTDTC, SESTDTC),
    SEENDY = study_day(RFSTDTC, SEENDTC),
    SEUPDES = NA_character_
  ) |>
  arrange(USUBJID, suppressWarnings(as.integer(TAETORD))) |>
  select(
    DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH,
    SESTDTC, SEENDTC, SESTDY, SEENDY, SEUPDES
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(se, "/app/output/se.csv", na = "")
