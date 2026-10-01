# Reference solution for the yamaa benchmark adam-adsl-alive-date (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl_raw <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTEDT = col_date(), .default = col_character())
)
adae <- read_csv(
  "/app/input/adae.csv",
  col_types = cols(AENDT = col_date(), .default = col_character())
)
advs <- read_csv(
  "/app/input/advs.csv",
  col_types = cols(ADATE = col_date(), .default = col_character())
)

# The contact text completed to a day: a full date stands, a year and month
# take the month's first day, a year alone takes January first, and anything
# else leaves the date missing.
adsl_raw <- adsl_raw |>
  mutate(
    .full = suppressWarnings(as.Date(LSTCNTDC, format = "%Y-%m-%d")),
    .ym = suppressWarnings(as.Date(paste0(LSTCNTDC, "-01"), format = "%Y-%m-%d")),
    .y = suppressWarnings(as.Date(paste0(LSTCNTDC, "-01-01"), format = "%Y-%m-%d")),
    LSTCNTDT = coalesce(.full, .ym, .y)
  )

# Each subject's latest adverse event end and vital signs dates.
adae_last <- adae |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    ADAE_LST = if (all(is.na(AENDT))) as.Date(NA) else max(AENDT, na.rm = TRUE),
    .groups = "drop"
  )
advs_last <- advs |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    ADVS_LST = if (all(is.na(ADATE))) as.Date(NA) else max(ADATE, na.rm = TRUE),
    .groups = "drop"
  )

adsl <- adsl_raw |>
  left_join(adae_last, by = c("STUDYID", "USUBJID")) |>
  left_join(advs_last, by = c("STUDYID", "USUBJID")) |>
  mutate(
    LSTALVDT = suppressWarnings(do.call(
      pmax,
      list(TRTEDT, LSTCNTDT, ADAE_LST, ADVS_LST, na.rm = TRUE)
    ))
  ) |>
  mutate(
    LSTALVDT = if_else(
      is.na(TRTEDT) & is.na(LSTCNTDT) & is.na(ADAE_LST) & is.na(ADVS_LST),
      as.Date(NA),
      LSTALVDT
    )
  ) |>
  select(STUDYID, USUBJID, TRTEDT, LSTCNTDC, LSTCNTDT, LSTALVDT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
