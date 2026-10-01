# Reference solution for the yamaa benchmark sdtm-dm-arms (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

rand <- read_csv(
  "/app/input/rand.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(.default = col_character())
)

# The planned arm follows randomization; the actual arm follows exposure.
# PLACEBO was the PBO arm and VITAMIN D3 the TRT arm.
exposure <- ex |>
  filter(!is.na(EXTRT) & EXTRT != "") |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, EXTRT)

dm <- rand |>
  select(STUDYID, USUBJID, RANDCD, RAND, SCRNFL) |>
  full_join(exposure, by = c("STUDYID", "USUBJID")) |>
  mutate(
    RANDCD = na_if(str_trim(RANDCD), ""),
    RAND = na_if(str_trim(RAND), ""),
    EXTRT = na_if(str_trim(EXTRT), ""),
    SCRNFL = coalesce(str_trim(SCRNFL), ""),
    # Never randomized leaves the planned arm empty; never treated or a
    # treatment matching no planned arm leaves the actual arm empty.
    ARMCD = if_else(is.na(RANDCD), NA_character_, RANDCD),
    ARM = if_else(is.na(RANDCD), NA_character_, RAND),
    ACTARMCD = case_when(
      EXTRT == "PLACEBO" ~ "PBO",
      EXTRT == "VITAMIN D3" ~ "TRT",
      .default = NA_character_
    ),
    ACTARM = case_when(
      EXTRT == "PLACEBO" ~ "Placebo",
      EXTRT == "VITAMIN D3" ~ "Vitamin D3",
      .default = NA_character_
    ),
    # A treated subject carries the other planned arm when that is what
    # the exposure says; the reason for a blank arm depends on why both
    # are blank, or on never being treated.
    ARMNRS = case_when(
      is.na(ARM) & is.na(ACTARM) & SCRNFL == "Y" ~ "SCREEN FAILURE",
      is.na(ARM) & is.na(ACTARM) ~ "NOT ASSIGNED",
      !is.na(ARM) & is.na(ACTARM) & is.na(EXTRT) ~ "NOT TREATED",
      .default = NA_character_
    ),
    ACTARMUD = if_else(
      !is.na(EXTRT) & is.na(ACTARM), EXTRT, NA_character_, missing = NA_character_
    ),
    DOMAIN = "DM"
  ) |>
  arrange(USUBJID) |>
  select(
    DOMAIN, STUDYID, USUBJID, ARMCD, ARM,
    ACTARMCD, ACTARM, ARMNRS, ACTARMUD
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
