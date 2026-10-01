# Reference solution for the yamaa benchmark sdtm-dm-age (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(stringr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

is_leap <- function(year) {
  year %% 4 == 0 & (year %% 100 != 0 | year %% 400 == 0)
}

whole_years <- function(birth, consent) {
  full <- str_detect(birth, "^\\d{4}-\\d{2}-\\d{2}$") &
    str_detect(consent, "^\\d{4}-\\d{2}-\\d{2}$")
  by <- as.integer(str_sub(birth, 1, 4))
  bm <- as.integer(str_sub(birth, 6, 7))
  bd <- as.integer(str_sub(birth, 9, 10))
  cy <- as.integer(str_sub(consent, 1, 4))
  cm <- as.integer(str_sub(consent, 6, 7))
  cd <- as.integer(str_sub(consent, 9, 10))
  # A February 29 birthday has its anniversary on February 28 in a
  # common year. A year-month or year-only birth date has no age.
  ann_m <- if_else(bm == 2 & bd == 29 & !is_leap(cy), 2L, bm)
  ann_d <- if_else(bm == 2 & bd == 29 & !is_leap(cy), 28L, bd)
  age <- cy - by - if_else(
    cm < ann_m | (cm == ann_m & cd < ann_d), 1L, 0L
  )
  if_else(full, age, NA_integer_)
}

# Whole calendar years between the birth date and the consent date. A
# missing birth date leaves the birth date, age, and units empty.
dm <- odm |>
  filter(ItemGroupOID == "IG.DM") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    DOMAIN = "DM",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    RFICDTC = na_if(`IT.DM.RFICDTC`, ""),
    BRTHDTC = na_if(`IT.DM.BRTHDT`, ""),
    AGE = whole_years(BRTHDTC, RFICDTC),
    AGEU = if_else(is.na(AGE), NA_character_, "YEARS")
  ) |>
  arrange(USUBJID) |>
  select(DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
