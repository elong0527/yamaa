# Reference solution for the yamaa benchmark sdtm-lb-lab-specific-ranges (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(purrr)
library(lubridate)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
) |>
  mutate(Value = na_if(Value, ""))

lbrange <- read_csv(
  "/app/input/lbrange.csv",
  col_types = cols(
    AGELO = col_integer(), AGEHI = col_integer(),
    NRLO = col_double(), NRHI = col_double(),
    .default = col_character()
  )
)

age_years <- function(birth, collected) {
  b <- ymd(birth)
  c <- ymd(collected)
  years <- year(c) - year(b)
  before_birthday <- (month(c) < month(b)) |
    (month(c) == month(b) & day(c) < day(b))
  years - if_else(before_birthday, 1L, 0L)
}

dm <- odm |>
  filter(ItemGroupOID == "IG.DM") |>
  select(SubjectKey, ItemOID, Value) |>
  pivot_wider(names_from = ItemOID, values_from = Value)

labs <- odm |>
  filter(ItemGroupOID == "IG.LB") |>
  distinct(StudyOID, SubjectKey, ItemGroupRepeatKey)

one_lab <- function(study, subj, igrep) {
  sub <- odm |>
    filter(
      SubjectKey == subj, ItemGroupOID == "IG.LB",
      ItemGroupRepeatKey == igrep
    )
  get <- function(oid) {
    hit <- sub |> filter(ItemOID == oid)
    if (nrow(hit) == 0) return(NA_character_)
    hit$Value[[1]]
  }
  dsub <- dm |> filter(SubjectKey == subj)
  sex <- if (nrow(dsub) == 0) NA_character_ else dsub$`IT.DM.SEX`[[1]]
  birth <- if (nrow(dsub) == 0) NA_character_ else dsub$`IT.DM.BRTHDTC`[[1]]
  code <- get("IT.LB.LBTESTCD")
  lbnam <- get("IT.LB.LBNAM")
  lbdtc <- get("IT.LB.LBDTC")
  raw <- get("IT.LB.LBSTRESN")
  stresn <- suppressWarnings(as.numeric(raw))
  unit <- NA_character_
  lo <- NA_real_
  hi <- NA_real_
  if (!is.na(birth) && !is.na(lbdtc) && !is.na(code) && !is.na(lbnam) && !is.na(sex)) {
    age <- age_years(birth, lbdtc)
    hit <- lbrange |>
      filter(
        LBNAM == lbnam, LBTESTCD == code, SEX == sex,
        AGELO <= age, age <= AGEHI,
        EFFSTDT <= lbdtc,
        is.na(EFFENDT) | lbdtc <= EFFENDT
      ) |>
      slice_head(n = 1)
    if (nrow(hit) == 1) {
      unit <- hit$UNIT[[1]]
      lo <- hit$NRLO[[1]]
      hi <- hit$NRHI[[1]]
    }
  }
  nrind <- if (is.na(stresn) || is.na(lo) || is.na(hi)) {
    NA_character_
  } else if (stresn < lo) {
    "LOW"
  } else if (stresn > hi) {
    "HIGH"
  } else {
    "NORMAL"
  }
  tibble(
    DOMAIN = "LB",
    STUDYID = study,
    USUBJID = subj,
    LBSEQ = as.integer(igrep),
    LBTESTCD = code,
    SEX = sex,
    LBNAM = lbnam,
    LBSTRESN = stresn,
    LBORRESU = unit,
    LBSTNRLO = lo,
    LBSTNRHI = hi,
    LBNRIND = nrind
  )
}

lb <- pmap_dfr(
  list(labs$StudyOID, labs$SubjectKey, labs$ItemGroupRepeatKey),
  one_lab
) |>
  arrange(STUDYID, USUBJID, LBSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
