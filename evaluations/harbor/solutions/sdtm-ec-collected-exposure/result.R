# Reference solution for the yamaa benchmark sdtm-ec-collected-exposure (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

log <- read_csv(
  "/app/input/dosing_log.csv",
  col_types = cols(.default = col_character())
)

ec <- log |>
  mutate(
    DOMAIN = "EC",
    ECTRT = "Study Drug",
    ECMOOD = "PERFORMED",
    ECOCCUR = DOSE_TAKEN,
    ECREASND = ifelse(DOSE_TAKEN == "Y", NA_character_, na_if(MISSREAS, "")),
    ECDOSE = suppressWarnings(as.integer(ifelse(DOSE_TAKEN == "Y", TABLETS, NA_character_))),
    ECDOSU = "tablet",
    ECDOSFRM = "TABLET",
    ECDOSFRQ = "QD",
    ECROUTE = "ORAL",
    ECSTDTC = suppressWarnings(as.Date(LOGDATE)),
    ECENDTC = suppressWarnings(as.Date(LOGDATE)),
    ECADJ = na_if(DOSE_ADJ, "")
  ) |>
  arrange(STUDYID, USUBJID, ECSTDTC) |>
  group_by(STUDYID, USUBJID) |>
  mutate(ECSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND,
    ECDOSE, ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ec, "/app/output/ec.csv", na = "")
