# Reference solution for the yamaa benchmark adam-adlb-bds (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
)
lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    LBDTC = col_date(),
    LBSTRESN = col_double(),
    .default = col_character()
  )
)

# One record per collected ALT/AST result; a collected result with no
# numeric value produces no record.
collected <- lb |>
  filter(LBTESTCD %in% c("ALT", "AST"), !is.na(LBSTRESN))

# ALT and AST hold the collected result and unit; ALTSI holds the ALT
# result times 0.0167 with unit ukat/L, one record for each ALT record.
alt <- collected |>
  filter(LBTESTCD %in% "ALT") |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "ALT",
    PARAM = "Alanine Aminotransferase",
    ADT = LBDTC,
    AVAL = LBSTRESN,
    AVALU = LBSTRESU
  )
ast <- collected |>
  filter(LBTESTCD %in% "AST") |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "AST",
    PARAM = "Aspartate Aminotransferase",
    ADT = LBDTC,
    AVAL = LBSTRESN,
    AVALU = LBSTRESU
  )
altsi <- collected |>
  filter(LBTESTCD %in% "ALT") |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = "ALTSI",
    PARAM = "Alanine Aminotransferase (SI)",
    ADT = LBDTC,
    AVAL = LBSTRESN * 0.0167,
    AVALU = "ukat/L"
  )

base <- bind_rows(alt, ast, altsi) |>
  left_join(
    adsl |> select(STUDYID, USUBJID, TRTSDT, TRT01A),
    by = c("STUDYID", "USUBJID")
  )

# ABLFL is Y on the latest record on or before the treatment start date
# for each subject and parameter, including the start date itself.
baseline_dates <- base |>
  filter(ADT <= TRTSDT) |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  summarise(BASE_DT = max(ADT), .groups = "drop")

with_flag <- base |>
  left_join(baseline_dates, by = c("STUDYID", "USUBJID", "PARAMCD")) |>
  mutate(ABLFL = if_else(!is.na(BASE_DT) & ADT == BASE_DT, "Y", NA_character_))

# BASE repeats the flagged baseline value; CHG and PCHG follow, with PCHG
# empty when the baseline is zero. Values are not rounded.
baselines <- with_flag |>
  filter(ABLFL %in% "Y") |>
  select(STUDYID, USUBJID, PARAMCD, BASE = AVAL)

adlb_flagged <- with_flag |>
  select(-BASE_DT) |>
  left_join(baselines, by = c("STUDYID", "USUBJID", "PARAMCD")) |>
  mutate(
    CHG = AVAL - BASE,
    PCHG = if_else(
      is.na(BASE) | BASE == 0,
      NA_real_,
      100 * (AVAL - BASE) / BASE
    )
  )

# ASEQ numbers each subject's records from 1, ordered by PARAMCD (ALT,
# then ALTSI, then AST) and then by ADT.
param_order <- c("ALT" = 0L, "ALTSI" = 1L, "AST" = 2L)

adlb <- adlb_flagged |>
  mutate(PARAM_ORDER = param_order[PARAMCD]) |>
  arrange(STUDYID, USUBJID, PARAM_ORDER, ADT) |>
  group_by(STUDYID, USUBJID) |>
  mutate(ASEQ = row_number()) |>
  ungroup() |>
  arrange(PARAM_ORDER, USUBJID, ADT) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A,
    AVAL, AVALU, ABLFL, BASE, CHG, PCHG, ASEQ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
