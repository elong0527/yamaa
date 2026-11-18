# Reference solution for the yamaa benchmark adam-adsl-disposition (R track).
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
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(DSSEQ = col_integer(), DSSTDTC = col_date(), .default = col_character())
)

# The last disposition event per track: latest date, highest sequence on a
# tie, and an undated event only when the track has no dated one (arrange
# puts missing dates last). The study track reads every epoch but
# TREATMENT; the treatment track reads only TREATMENT.
last_study <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH != "TREATMENT") |>
  arrange(STUDYID, USUBJID, desc(DSSTDTC), desc(DSSEQ)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, EOSDT = DSSTDTC, EOSDECOD = DSDECOD, EOSREAS = DSTERM)

last_trt <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH %in% "TREATMENT") |>
  arrange(STUDYID, USUBJID, desc(DSSTDTC), desc(DSSEQ)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, EOTDT = DSSTDTC, EOTDECOD = DSDECOD, EOTREAS = DSTERM)

adsl <- dm |>
  left_join(last_study, by = c("STUDYID", "USUBJID")) |>
  left_join(last_trt, by = c("STUDYID", "USUBJID")) |>
  mutate(
    # A screen failure never entered the study, so it has no status.
    EOSSTT = case_when(
      EOSDECOD %in% "SCREEN FAILURE" ~ NA_character_,
      EOSDECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(EOSDECOD) ~ "DISCONTINUED",
      .default = "ONGOING"
    ),
    DCSREAS = if_else(EOSSTT %in% "DISCONTINUED", EOSDECOD, NA_character_),
    # A subject never treated has no treatment status.
    EOTSTT = case_when(
      is.na(RFXSTDTC) ~ NA_character_,
      EOTDECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(EOTDECOD) ~ "DISCONTINUED",
      .default = "ONGOING"
    ),
    DCTREAS = if_else(EOTSTT %in% "DISCONTINUED", EOTDECOD, NA_character_),
    DCTREASP = if_else(DCTREAS %in% "OTHER", EOTREAS, NA_character_)
  ) |>
  select(
    STUDYID, USUBJID, EOSDT, EOSREAS, EOSSTT, DCSREAS,
    EOTDT, EOTSTT, DCTREAS, DCTREASP
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
