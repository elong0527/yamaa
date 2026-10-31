# The bds case in dplyr: cases/bds/spec.yaml's derivation, written the way a
# dplyr user would write it, reading and writing Parquet with arrow.
#
# Usage: Rscript dplyr_bds.R DIRECTORY. Reads DIRECTORY/input/lb.parquet and
# DIRECTORY/input/adsl.parquet and writes DIRECTORY/adlb.parquet.

suppressPackageStartupMessages({
  library(dplyr)
  library(arrow)
})

directory <- commandArgs(trailingOnly = TRUE)[[1]]
lb <- read_parquet(file.path(directory, "input", "lb.parquet"))
adsl <- read_parquet(file.path(directory, "input", "adsl.parquet"))

alt <- filter(lb, LBTESTCD == "ALT", !is.na(LBSTRESN))
ast <- filter(lb, LBTESTCD == "AST", !is.na(LBSTRESN))
rows <- bind_rows(
  mutate(alt, PARAMCD = "ALT", PARAM = "Alanine Aminotransferase",
         AVAL = LBSTRESN, AVALU = LBSTRESU),
  mutate(alt, PARAMCD = "ALTSI", PARAM = "Alanine Aminotransferase (SI)",
         AVAL = LBSTRESN * 0.0167, AVALU = "ukat/L"),
  mutate(ast, PARAMCD = "AST", PARAM = "Aspartate Aminotransferase",
         AVAL = LBSTRESN, AVALU = LBSTRESU)
)

adlb <- rows |>
  left_join(select(adsl, STUDYID, USUBJID, TRTSDT, TRT01A),
            by = c("STUDYID", "USUBJID")) |>
  mutate(ADT = LBDTC) |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  # Baseline: the latest record on or before treatment start with a value.
  mutate(
    eligible = ADT <= TRTSDT & !is.na(AVAL),
    latest = if (any(eligible)) max(ADT[eligible]) else as.Date(NA),
    ABLFL = if_else(eligible & ADT == latest, "Y", NA_character_),
    BASE = if (sum(ABLFL == "Y", na.rm = TRUE) == 1)
      AVAL[which(ABLFL == "Y")] else NA_real_
  ) |>
  ungroup() |>
  mutate(CHG = AVAL - BASE,
         PCHG = 100 * CHG / if_else(BASE == 0, NA_real_, BASE)) |>
  arrange(STUDYID, USUBJID, PARAMCD, ADT) |>
  group_by(STUDYID, USUBJID) |>
  mutate(ASEQ = row_number()) |>
  ungroup() |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU,
         ABLFL, BASE, CHG, PCHG, ASEQ)

write_parquet(adlb, file.path(directory, "adlb.parquet"))
