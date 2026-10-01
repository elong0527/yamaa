# Reference solution for the yamaa benchmark adam-adsl-dose-adjustment (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

base <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(.default = col_character())
)
ec <- read_csv(
  "/app/input/ec.csv",
  col_types = cols(.default = col_character())
)
fa <- read_csv(
  "/app/input/fa.csv",
  col_types = cols(.default = col_character())
)

# A record reports an adjustment with a filled value; findings-about rows
# qualify only for dose adjustment occurrences with a Y result.
ex_adj <- ex |>
  filter(!is.na(EXADJ), str_trim(EXADJ) != "") |>
  distinct(STUDYID, USUBJID) |>
  mutate(.ex_adj = TRUE)
ec_adj <- ec |>
  filter(!is.na(ECADJ), str_trim(ECADJ) != "") |>
  distinct(STUDYID, USUBJID) |>
  mutate(.ec_adj = TRUE)
fa_adj <- fa |>
  filter(FATESTCD %in% "OCCUR", FAOBJ %in% "DOSE ADJUSTMENT", FASTRESC %in% "Y") |>
  distinct(STUDYID, USUBJID) |>
  mutate(.fa_adj = TRUE)

ex_any <- ex |> distinct(STUDYID, USUBJID) |> mutate(.ex_any = TRUE)
ec_any <- ec |> distinct(STUDYID, USUBJID) |> mutate(.ec_any = TRUE)
fa_any <- fa |> distinct(STUDYID, USUBJID) |> mutate(.fa_any = TRUE)

adsl <- base |>
  left_join(ex_adj, by = c("STUDYID", "USUBJID")) |>
  left_join(ec_adj, by = c("STUDYID", "USUBJID")) |>
  left_join(fa_adj, by = c("STUDYID", "USUBJID")) |>
  left_join(ex_any, by = c("STUDYID", "USUBJID")) |>
  left_join(ec_any, by = c("STUDYID", "USUBJID")) |>
  left_join(fa_any, by = c("STUDYID", "USUBJID")) |>
  mutate(
    DOSADJFL = case_when(
      .ex_adj %in% TRUE | .ec_adj %in% TRUE | .fa_adj %in% TRUE ~ "Y",
      .ex_any %in% TRUE | .ec_any %in% TRUE | .fa_any %in% TRUE ~ "N"
    )
  ) |>
  select(STUDYID, USUBJID, DOSADJFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
