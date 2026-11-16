# Reference solution for the yamaa benchmark adam-adsl-rescue-med (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)

# A partial start completes to its first possible day: a year and month to
# the first of the month (flag D), a year alone to January 1 (flag M).
first <- cm |>
  filter(CMCAT == "RESCUE MEDICATION") |>
  mutate(
    RESCDTF = case_when(
      nchar(CMSTDTC) == 4 ~ "M",
      nchar(CMSTDTC) == 7 ~ "D",
      .default = NA_character_
    ),
    RESCDT = as.Date(case_when(
      nchar(CMSTDTC) == 4 ~ paste0(CMSTDTC, "-01-01"),
      nchar(CMSTDTC) == 7 ~ paste0(CMSTDTC, "-01"),
      .default = CMSTDTC
    ))
  ) |>
  # arrange puts a missing date last, so an undated record is first only
  # when the subject has no dated one.
  arrange(STUDYID, USUBJID, RESCDT, CMSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, RESCDT, RESCDTF, RESCTRT = CMDECOD)

adsl <- dm |>
  left_join(first, by = c("STUDYID", "USUBJID")) |>
  mutate(RESCFL = if_else(is.na(RESCTRT), "N", "Y")) |>
  select(STUDYID, USUBJID, RESCFL, RESCDT, RESCDTF, RESCTRT) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
