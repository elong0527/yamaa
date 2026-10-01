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

# The last dated disposition event: latest date, highest sequence on a tie.
last <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", !is.na(DSSTDTC)) |>
  arrange(STUDYID, USUBJID, desc(DSSTDTC), desc(DSSEQ)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, EOSDT = DSSTDTC, EOSDECOD = DSDECOD, EOSREAS = DSTERM)

adsl <- dm |>
  left_join(last, by = c("STUDYID", "USUBJID")) |>
  mutate(
    EOSSTT = case_when(
      EOSDECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(EOSDECOD) ~ "DISCONTINUED",
      .default = "ONGOING"
    ),
    DCSREAS = if_else(EOSSTT == "DISCONTINUED", EOSREAS, NA_character_)
  ) |>
  select(STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
