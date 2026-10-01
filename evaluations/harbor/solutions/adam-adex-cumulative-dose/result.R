# Reference solution for the yamaa benchmark adam-adex-cumulative-dose (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)
trt <- read_csv(
  "/app/input/subject_treatment.csv",
  col_types = cols(
    PLANDOSE = col_double(), PLANCYC = col_integer(), .default = col_character()
  )
)

# Every exposure record counts, even one with a zero or missing dose; a
# missing dose adds nothing. A duplicated record counts once per entry.
doses <- ex |>
  mutate(EXDOSE_NUM = suppressWarnings(as.numeric(EXDOSE))) |>
  group_by(USUBJID, EXTRT) |>
  summarise(
    DOSECUM = sum(EXDOSE_NUM, na.rm = TRUE),
    NCYCLES = n(),
    .groups = "drop"
  )

adex <- trt |>
  left_join(doses, by = c("USUBJID", "EXTRT")) |>
  mutate(
    PLANTOT = PLANDOSE * PLANCYC,
    # Not rounded; no value when the planned total is zero or when there is
    # no cumulative dose to compare.
    RDI = case_when(
      is.na(DOSECUM) ~ NA_real_,
      is.na(PLANTOT) | PLANTOT == 0 ~ NA_real_,
      .default = DOSECUM / PLANTOT * 100
    )
  ) |>
  select(STUDYID, USUBJID, EXTRT, EXDOSU, DOSECUM, NCYCLES, RDI)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adex, "/app/output/adex.csv", na = "")
