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

# The last dated disposition event per track: latest date, highest sequence
# on a tie. The study track reads every epoch but TREATMENT; the treatment
# track reads only TREATMENT.
last_study <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH != "TREATMENT", !is.na(DSSTDTC)) |>
  arrange(STUDYID, USUBJID, desc(DSSTDTC), desc(DSSEQ)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, EOSDT = DSSTDTC, EOSDECOD = DSDECOD, EOSREAS = DSTERM)

last_trt <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH %in% "TREATMENT", !is.na(DSSTDTC)) |>
  arrange(STUDYID, USUBJID, desc(DSSTDTC), desc(DSSEQ)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, EOTDT = DSSTDTC, EOTDECOD = DSDECOD, EOTREAS = DSTERM)

adsl <- dm |>
  left_join(last_study, by = c("STUDYID", "USUBJID")) |>
  left_join(last_trt, by = c("STUDYID", "USUBJID")) |>
  mutate(
    EOSSTT = case_when(
      EOSDECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(EOSDECOD) ~ "DISCONTINUED",
      .default = "ONGOING"
    ),
    DCSREAS = if_else(EOSSTT == "DISCONTINUED", EOSREAS, NA_character_),
    EOTSTT = case_when(
      EOTDECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(EOTDT) ~ "DISCONTINUED",
      .default = "ONGOING"
    ),
    DCTREAS = if_else(EOTSTT == "DISCONTINUED", EOTREAS, NA_character_),
    DCSREASP = if_else(EOTDECOD %in% "OTHER", EOTREAS, NA_character_)
  ) |>
  select(
    STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS,
    EOTDT, EOTSTT, DCTREAS, DCSREASP
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
