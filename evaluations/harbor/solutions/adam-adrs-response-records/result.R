# Reference solution for the yamaa benchmark adam-adrs-response-records (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(readr)
library(stringr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
)
rs <- read_csv(
  "/app/input/rs.csv",
  col_types = cols(RSSEQ = col_integer(), .default = col_character())
)

# Only investigator overall response assessments leave a record.
base <- rs |>
  filter(RSTESTCD %in% "OVRLRESP", RSEVAL %in% "INVESTIGATOR") |>
  left_join(select(adsl, USUBJID, TRTSDT), by = "USUBJID") |>
  mutate(
    PARAMCD = "OVR",
    PARAM = "Overall Response by Investigator",
    RSDTC = RSDTC,
    ADT = case_when(
      str_detect(RSDTC, "^[0-9]{4}-[0-9]{2}-[0-9]{2}$") ~ suppressWarnings(ymd(RSDTC, quiet = TRUE)),
      str_detect(RSDTC, "^[0-9]{4}-[0-9]{2}$") ~ suppressWarnings(ymd(paste0(RSDTC, "-01"), quiet = TRUE)),
      str_detect(RSDTC, "^[0-9]{4}$") ~ suppressWarnings(ymd(paste0(RSDTC, "-01-01"), quiet = TRUE)),
      .default = as.Date(NA)
    ),
    ADY = case_when(
      is.na(ADT) | is.na(TRTSDT) ~ NA_integer_,
      ADT >= TRTSDT ~ as.integer(ADT - TRTSDT) + 1L,
      .default = as.integer(ADT - TRTSDT)
    ),
    AVALC = RSSTRESC,
    AVAL = case_when(
      AVALC == "CR" ~ 1L,
      AVALC == "PR" ~ 2L,
      AVALC == "SD" ~ 3L,
      AVALC == "NON-CR/NON-PD" ~ 4L,
      AVALC == "PD" ~ 5L,
      AVALC == "NE" ~ 6L
    )
  )

# One flagged record at each assessment date: the worst response that day,
# with the lowest sequence number breaking ties.
flagged <- base |>
  group_by(STUDYID, USUBJID, ADT) |>
  arrange(desc(AVAL), RSSEQ, .by_group = TRUE) |>
  mutate(
    rank = row_number(),
    ANL01FL = if_else(rank == 1L, "Y", NA_character_)
  ) |>
  ungroup() |>
  select(-rank)

adrs <- flagged |>
  select(
    STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY,
    AVALC, AVAL, ANL01FL
  ) |>
  arrange(USUBJID, ADT, RSSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adrs, "/app/output/adrs.csv", na = "")
