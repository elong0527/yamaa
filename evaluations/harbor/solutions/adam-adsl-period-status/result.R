# Reference solution for the yamaa benchmark adam-adsl-period-status (R track).
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
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(DSSEQ = col_integer(), .default = col_character())
)

# A subject entered a period when it has an exposure record in that
# period's epoch; its disposition event is the DISPOSITION EVENT record
# in the same epoch.
entered1 <- ex |>
  filter(EPOCH %in% "TREATMENT 1") |>
  distinct(STUDYID, USUBJID) |>
  mutate(P01EX = 1L)
entered2 <- ex |>
  filter(EPOCH %in% "TREATMENT 2") |>
  distinct(STUDYID, USUBJID) |>
  mutate(P02EX = 1L)

event1 <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH %in% "TREATMENT 1") |>
  transmute(STUDYID, USUBJID, P01DECOD = DSDECOD)
event2 <- ds |>
  filter(DSCAT %in% "DISPOSITION EVENT", EPOCH %in% "TREATMENT 2") |>
  transmute(STUDYID, USUBJID, P02DECOD = DSDECOD)

adsl <- dm |>
  left_join(entered1, by = c("STUDYID", "USUBJID")) |>
  left_join(entered2, by = c("STUDYID", "USUBJID")) |>
  left_join(event1, by = c("STUDYID", "USUBJID")) |>
  left_join(event2, by = c("STUDYID", "USUBJID")) |>
  mutate(
    EOP01STT = case_when(
      !is.na(P01EX) & !is.na(P01DECOD) & P01DECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(P01EX) & !is.na(P01DECOD) ~ "DISCONTINUED",
      !is.na(P01EX) ~ "ONGOING",
      .default = NA_character_
    ),
    DCP01RS = if_else(EOP01STT %in% "DISCONTINUED", P01DECOD, NA_character_),
    EOP02STT = case_when(
      !is.na(P02EX) & !is.na(P02DECOD) & P02DECOD == "COMPLETED" ~ "COMPLETED",
      !is.na(P02EX) & !is.na(P02DECOD) ~ "DISCONTINUED",
      !is.na(P02EX) ~ "ONGOING",
      .default = NA_character_
    ),
    DCP02RS = if_else(EOP02STT %in% "DISCONTINUED", P02DECOD, NA_character_)
  ) |>
  select(STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
