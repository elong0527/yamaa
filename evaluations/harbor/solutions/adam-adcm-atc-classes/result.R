# Reference solution for the yamaa benchmark adam-adcm-atc-classes (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)
facm <- read_csv(
  "/app/input/facm.csv",
  col_types = cols(
    FASEQ = col_integer(), CMSEQ = col_integer(), .default = col_character()
  )
)
atcdict <- read_csv(
  "/app/input/atc_dict.csv",
  col_types = cols(.default = col_character())
)

# One ATC level per finding row; rows naming no level from ATC1 to ATC4, or
# pointing at no medication record, change nothing.
levels <- c("ATC1", "ATC2", "ATC3", "ATC4")
codes <- facm |>
  filter(FATESTCD %in% levels) |>
  mutate(code_col = paste0(FATESTCD, "CD")) |>
  select(USUBJID, CMSEQ, code_col, FAORRES) |>
  pivot_wider(names_from = code_col, values_from = FAORRES) |>
  mutate(CMSEQ = as.integer(CMSEQ))

lookup <- atcdict |> select(ATCCODE, ATCNAME)

adcm <- cm |>
  left_join(codes, by = c("USUBJID", "CMSEQ")) |>
  # A medication with no coded name keeps every ATC column empty.
  mutate(
    across(
      c(ATC1CD, ATC2CD, ATC3CD, ATC4CD),
      ~ if_else(is.na(CMDECOD) | CMDECOD == "", NA_character_, .x)
    )
  ) |>
  left_join(lookup |> rename(ATC1CD = ATCCODE, ATC1 = ATCNAME), by = "ATC1CD") |>
  left_join(lookup |> rename(ATC2CD = ATCCODE, ATC2 = ATCNAME), by = "ATC2CD") |>
  left_join(lookup |> rename(ATC3CD = ATCCODE, ATC3 = ATCNAME), by = "ATC3CD") |>
  left_join(lookup |> rename(ATC4CD = ATCCODE, ATC4 = ATCNAME), by = "ATC4CD") |>
  select(
    STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, ATC1, ATC2, ATC3, ATC4,
    ATC1CD, ATC2CD, ATC3CD, ATC4CD
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adcm, "/app/output/adcm.csv", na = "")
