# Reference solution for the yamaa benchmark adam-adppk-baseline-covariates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(tidyr)
  library(readr)
  library(stringr)
})

read_src <- function(name) {
  read_csv(
    file.path("/app/input", name),
    col_types = cols(.default = col_character()),
    na = c("", "NA"),
    trim_ws = TRUE,
    progress = FALSE
  )
}

adsl <- read_src("adsl.csv")
ex <- read_src("ex.csv")
lb <- read_src("lb.csv")
pc <- read_src("pc.csv")
vs <- read_src("vs.csv")

with_seconds <- function(x) {
  date_part <- "[0-9]{4}-[0-9]{2}-[0-9]{2}"
  case_when(
    is.na(x) ~ NA_character_,
    str_detect(x, paste0("^", date_part, "T[0-9]{2}:[0-9]{2}$")) ~ paste0(x, ":00"),
    str_detect(x, paste0("^", date_part, "T[0-9]{2}:[0-9]{2}:[0-9]{2}")) ~ str_sub(x, 1, 19),
    TRUE ~ x
  )
}

baseline <- function(data, testcd, flag, value, tests) {
  data %>%
    transmute(
      USUBJID,
      TESTCD = .data[[testcd]],
      FLAG = .data[[flag]],
      VALUE = as.numeric(.data[[value]])
    ) %>%
    filter(FLAG %in% "Y", TESTCD %in% names(tests)) %>%
    group_by(USUBJID, TESTCD) %>%
    slice(1) %>%
    ungroup() %>%
    mutate(TESTCD = factor(unname(tests[TESTCD]), levels = unname(tests))) %>%
    select(USUBJID, TESTCD, VALUE) %>%
    pivot_wider(
      names_from = TESTCD,
      values_from = VALUE,
      names_expand = TRUE
    )
}

vs_bl <- baseline(
  vs, "VSTESTCD", "VSBLFL", "VSSTRESN",
  c(WEIGHT = "WTBL", HEIGHT = "HTBL")
)
lb_bl <- baseline(
  lb, "LBTESTCD", "LBBLFL", "LBSTRESN",
  c(CREAT = "CREATBL", ALT = "ALTBL", BILI = "TBILBL")
)

events <- bind_rows(
  ex %>% transmute(USUBJID, ADTM = with_seconds(EXSTDTC), EVID = 1L),
  pc %>% transmute(USUBJID, ADTM = with_seconds(PCDTC), EVID = 0L)
)

adppk <- events %>%
  inner_join(
    adsl %>% transmute(STUDYID, USUBJID, AGE = as.numeric(AGE), SEX, RACE),
    by = "USUBJID"
  ) %>%
  left_join(vs_bl, by = "USUBJID") %>%
  left_join(lb_bl, by = "USUBJID") %>%
  arrange(USUBJID, ADTM, desc(EVID)) %>%
  group_by(USUBJID) %>%
  mutate(ASEQ = row_number()) %>%
  ungroup() %>%
  mutate(
    SEXN = case_when(SEX == "M" ~ 1L, SEX == "F" ~ 2L, TRUE ~ NA_integer_),
    RACEN = case_when(
      RACE %in% "ASIAN" ~ 2L,
      RACE %in% "BLACK OR AFRICAN AMERICAN" ~ 3L,
      RACE %in% "WHITE" ~ 5L,
      TRUE ~ 6L
    ),
    BMIBL = WTBL / (HTBL / 100)^2,
    BSABL = sqrt(HTBL * WTBL / 3600),
    CRCLBL = (140 - AGE) * WTBL *
      case_when(SEX == "M" ~ 1.23, SEX == "F" ~ 1.04, TRUE ~ NA_real_) /
      CREATBL,
    EGFR_R = CREATBL / 88.42 /
      case_when(SEX == "M" ~ 0.9, SEX == "F" ~ 0.7, TRUE ~ NA_real_),
    EGFR_A = case_when(
      EGFR_R > 1 ~ -1.2,
      SEX == "M" ~ -0.302,
      SEX == "F" ~ -0.241,
      TRUE ~ NA_real_
    ),
    EGFRBL = 142 * EGFR_R^EGFR_A * 0.9938^AGE *
      case_when(SEX == "F" ~ 1.012, SEX == "M" ~ 1, TRUE ~ NA_real_)
  ) %>%
  select(
    STUDYID, USUBJID, ASEQ, ADTM, EVID, AGE, SEX, SEXN, RACE, RACEN,
    WTBL, HTBL, BMIBL, BSABL, CREATBL, ALTBL, TBILBL, CRCLBL, EGFRBL
  )

write_csv(adppk, file.path("/app/output", "adppk.csv"), na = "")
