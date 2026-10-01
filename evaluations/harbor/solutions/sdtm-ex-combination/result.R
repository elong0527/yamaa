# Reference solution for the yamaa benchmark sdtm-ex-combination (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ex_raw <- read_csv(
  "/app/input/ex_raw.csv",
  col_types = cols(.default = col_character())
)

ex <- ex_raw |>
  mutate(
    DOMAIN = "EX",
    EXDOSE = suppressWarnings(as.numeric(EXDOSE)),
    EXSTDTC_DATE = suppressWarnings(as.Date(EXSTDTC)),
    EXADJ = na_if(EXADJ, "")
  ) |>
  arrange(STUDYID, USUBJID, EXSTDTC_DATE, EXTRT) |>
  group_by(STUDYID, USUBJID) |>
  mutate(EXSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU,
    EXSTDTC, EXENDTC, EXADJ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ex, "/app/output/ex.csv", na = "")
