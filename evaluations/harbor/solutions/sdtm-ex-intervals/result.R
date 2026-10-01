# Reference solution for the yamaa benchmark sdtm-ex-intervals (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ec_raw <- read_csv(
  "/app/input/ec_raw.csv",
  col_types = cols(.default = col_character())
)

ex <- ec_raw |>
  mutate(
    EXDOSE_NUM = suppressWarnings(as.numeric(ECDOSE)),
    ECSTDTC_DT = suppressWarnings(as.Date(ECSTDTC)),
    ECADJ_CLEAN = na_if(ECADJ, "")
  ) |>
  group_by(STUDYID, USUBJID, ECTRT, EXDOSE_NUM, ECDOSU, ECDOSFRQ) |>
  summarise(
    START_DT = min(ECSTDTC_DT, na.rm = TRUE),
    END_DT = max(ECSTDTC_DT, na.rm = TRUE),
    EXADJ = {
      vals <- sort(unique(ECADJ_CLEAN[!is.na(ECADJ_CLEAN)]))
      if (length(vals) == 0) NA_character_ else vals[1]
    },
    .groups = "drop"
  ) |>
  mutate(
    DOMAIN = "EX",
    EXTRT = ECTRT,
    EXDOSE = EXDOSE_NUM,
    EXDOSU = ECDOSU,
    EXDOSFRQ = ECDOSFRQ,
    EXSTDTC = START_DT,
    EXENDTC = END_DT
  ) |>
  arrange(STUDYID, USUBJID, START_DT, EXTRT) |>
  group_by(STUDYID, USUBJID) |>
  mutate(EXSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU,
    EXDOSFRQ, EXSTDTC, EXENDTC, EXADJ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ex, "/app/output/ex.csv", na = "")
