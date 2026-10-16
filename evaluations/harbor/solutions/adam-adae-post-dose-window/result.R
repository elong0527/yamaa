# Reference solution for the yamaa benchmark adam-adae-post-dose-window (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)
library(stringr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)

# Datetimes collected without seconds are completed to the second so
# onsets and dose starts compare at the moment, to the second.
complete_seconds <- function(x) {
  if_else(
    !is.na(x) & str_detect(x, "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}$"),
    paste0(x, ":00"),
    x
  )
}

events <- ae |>
  mutate(
    ASTDT = if_else(is.na(AESTDTC), NA_character_, substr(AESTDTC, 1, 10)),
    ASTDTM = if_else(
      !is.na(AESTDTC) & nchar(AESTDTC) > 10,
      complete_seconds(AESTDTC),
      NA_character_
    )
  ) |>
  select(STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM)

# The subject's doses, numbered 1, 2, 3 in order of start.
doses <- ex |>
  mutate(START = complete_seconds(EXSTDTC)) |>
  arrange(STUDYID, USUBJID, START, EXSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(
    DOSENUM = row_number(),
    STARTM = ymd_hms(START, tz = "UTC"),
    ENDM = STARTM + hours(24)
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, DOSENUM, STARTM, ENDM)

# A window opens at the dose start and closes 24 hours later, the
# closing moment itself outside. When two windows hold the onset, the
# later dose is counted.
picked <- events |>
  mutate(ONSETM = ymd_hms(ASTDTM, tz = "UTC")) |>
  inner_join(doses, by = c("STUDYID", "USUBJID"), relationship = "many-to-many") |>
  filter(!is.na(ONSETM), ONSETM >= STARTM, ONSETM < ENDM) |>
  arrange(STUDYID, USUBJID, AESEQ, STARTM) |>
  group_by(STUDYID, USUBJID, AESEQ) |>
  slice_tail(n = 1) |>
  ungroup() |>
  select(STUDYID, USUBJID, AESEQ, DOSENUM)

adae <- events |>
  left_join(picked, by = c("STUDYID", "USUBJID", "AESEQ")) |>
  mutate(DOSE24FL = if_else(is.na(DOSENUM), NA_character_, "Y")) |>
  select(
    STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM, DOSE24FL, DOSENUM
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
