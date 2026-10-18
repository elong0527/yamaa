# Reference solution for the yamaa benchmark adam-adcm-end-date-bound (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)

cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)
adsl <- read_csv("/app/input/adsl.csv", col_types = cols(.default = col_character()))

# A full date stands as collected; a year and month runs to its last day,
# a year alone to December 31, never past a date of death that falls inside
# the collected month or year. When the whole collected month or year lies
# after death, the last day stands.
adcm <- cm |>
  left_join(select(adsl, STUDYID, USUBJID, DTHDT), by = c("STUDYID", "USUBJID")) |>
  mutate(
    LEN = nchar(CMENDTC),
    FIRST = case_when(
      LEN == 7 ~ ymd(paste0(CMENDTC, "-01"), quiet = TRUE),
      LEN == 4 ~ ymd(paste0(CMENDTC, "-01-01"), quiet = TRUE)
    ),
    LAST = case_when(
      LEN == 7 ~ ceiling_date(FIRST, "month") - days(1),
      LEN == 4 ~ ymd(paste0(CMENDTC, "-12-31"), quiet = TRUE)
    ),
    DEATH = ymd(DTHDT, quiet = TRUE),
    BOUNDED = case_when(
      is.na(DEATH) ~ LAST,
      DEATH < FIRST ~ LAST,
      TRUE ~ pmin(LAST, DEATH)
    ),
    AENDT = case_when(
      LEN == 10 ~ CMENDTC,
      LEN %in% c(4, 7) & !is.na(BOUNDED) ~ format(BOUNDED, "%Y-%m-%d"),
      TRUE ~ NA_character_
    ),
    AENDTF = case_when(
      is.na(AENDT) ~ NA_character_,
      LEN == 7 ~ "D",
      LEN == 4 ~ "M",
      TRUE ~ NA_character_
    )
  ) |>
  select(STUDYID, USUBJID, CMSEQ, CMTRT, CMENDTC, AENDT, AENDTF, DTHDT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adcm, "/app/output/adcm.csv", na = "")
