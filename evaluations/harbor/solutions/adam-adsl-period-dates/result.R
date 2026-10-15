# Reference solution for the yamaa benchmark adam-adsl-period-dates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)

dm <- read_csv("/app/input/dm.csv", col_types = cols(.default = col_character()))
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
) |>
  mutate(START = ymd(EXSTDTC, quiet = TRUE), END = ymd(EXENDTC, quiet = TRUE))

first_start <- function(epoch) {
  ex |>
    filter(EPOCH == epoch, !is.na(START)) |>
    group_by(USUBJID) |>
    summarise(FIRST = min(START), .groups = "drop")
}

# The last exposure end counts every record, with or without a start.
last_end <- ex |>
  filter(!is.na(END)) |>
  group_by(USUBJID) |>
  summarise(LASTEND = max(END), .groups = "drop")

adsl <- dm |>
  left_join(rename(first_start("TREATMENT 1"), AP01SDTN = FIRST), by = "USUBJID") |>
  left_join(rename(first_start("TREATMENT 2"), AP02SDTN = FIRST), by = "USUBJID") |>
  left_join(last_end, by = "USUBJID") |>
  mutate(
    FOLLOW = LASTEND + days(30),
    # Period one ends the day before period two starts; the last period a
    # subject entered ends 30 days after the last exposure end.
    AP01EDTN = case_when(
      is.na(LASTEND) ~ as.Date(NA),
      !is.na(AP02SDTN) ~ AP02SDTN - days(1),
      TRUE ~ FOLLOW
    ),
    AP02EDTN = if_else(is.na(AP02SDTN), as.Date(NA), FOLLOW),
    AP01SDT = format(AP01SDTN, "%Y-%m-%d"),
    AP01EDT = format(AP01EDTN, "%Y-%m-%d"),
    AP02SDT = format(AP02SDTN, "%Y-%m-%d"),
    AP02EDT = format(AP02EDTN, "%Y-%m-%d")
  ) |>
  select(STUDYID, USUBJID, AP01SDT, AP01EDT, AP02SDT, AP02EDT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
