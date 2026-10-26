# Reference solution for the yamaa benchmark adam-adsl-alive-date (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(stringr)
  library(lubridate)
})

input_dir <- "/app/input"
output_dir <- "/app/output"

read_domain <- function(name) {
  read_csv(
    file.path(input_dir, paste0(name, ".csv")),
    col_types = cols(.default = col_character()),
    na = character(),
    trim_ws = TRUE,
    progress = FALSE
  ) |>
    mutate(across(everything(), \(x) if_else(is.na(x) | x == "", NA_character_, str_trim(x))))
}

# Split an ISO 8601 date or date-time into year, month and day parts.
# Anything after the date (a time) is dropped.
parse_parts <- function(x) {
  m <- str_match(x, "^(\\d{4})(?:-(\\d{2})(?:-(\\d{2}))?)?(?:$|[T ])")
  tibble(
    year = as.integer(m[, 2]),
    month = as.integer(m[, 3]),
    day = as.integer(m[, 4])
  )
}

# Date a value counts on: a date-time on its date, a year and month on the
# first of that month, a year alone on 1 January.
to_date <- function(x) {
  p <- parse_parts(x)
  make_date(p$year, coalesce(p$month, 1L), coalesce(p$day, 1L))
}

dm <- read_domain("dm")
ae <- read_domain("ae")
ds <- read_domain("ds")
ex <- read_domain("ex")
ss <- read_domain("ss")
sv <- read_domain("sv")
vs <- read_domain("vs")

alive_dates <- bind_rows(
  ex |> transmute(STUDYID, USUBJID, DT = to_date(EXSTDTC)),
  ex |> transmute(STUDYID, USUBJID, DT = to_date(EXENDTC)),
  ae |> transmute(STUDYID, USUBJID, DT = to_date(AESTDTC)),
  ae |> transmute(STUDYID, USUBJID, DT = to_date(AEENDTC)),
  ds |>
    filter(!(str_to_upper(coalesce(DSDECOD, "")) %in% c("DEATH", "LOST TO FOLLOW-UP"))) |>
    transmute(STUDYID, USUBJID, DT = to_date(DSSTDTC)),
  sv |> transmute(STUDYID, USUBJID, DT = to_date(SVSTDTC)),
  vs |> transmute(STUDYID, USUBJID, DT = to_date(VSDTC)),
  ss |>
    filter(str_to_upper(coalesce(SSSTRESC, "")) == "ALIVE") |>
    transmute(STUDYID, USUBJID, DT = to_date(SSDTC))
) |>
  filter(!is.na(DT)) |>
  group_by(STUDYID, USUBJID) |>
  summarise(MAXALIVE = max(DT), .groups = "drop")

fmt_date <- function(d) if_else(is.na(d), NA_character_, format(d, "%Y-%m-%d"))

adsl <- dm |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  left_join(alive_dates, by = c("STUDYID", "USUBJID")) |>
  mutate(
    parts = parse_parts(DTHDTC),
    dth_year = parts$year,
    dth_month = parts$month,
    dth_day = parts$day,
    dth_base = make_date(dth_year, coalesce(dth_month, 1L), coalesce(dth_day, 1L)),
    dth_partial = !is.na(dth_year) & (is.na(dth_month) | is.na(dth_day)),
    # the latest alive date replaces the imputed day when it falls later
    # inside the collected month (or year, when only the year was collected)
    inside_collected = !is.na(MAXALIVE) & !is.na(dth_year) &
      year(MAXALIVE) == dth_year &
      (is.na(dth_month) | month(MAXALIVE) == dth_month),
    take_alive = dth_partial & inside_collected & !is.na(MAXALIVE) &
      !is.na(dth_base) & MAXALIVE > dth_base,
    DTHDT_D = if_else(take_alive, MAXALIVE, dth_base),
    DTHDTF = case_when(
      dth_partial & is.na(dth_month) ~ "M",
      dth_partial ~ "D",
      TRUE ~ NA_character_
    ),
    DTHFL = if_else(
      str_to_upper(coalesce(DTHFL, "")) == "Y" | !is.na(dth_year),
      "Y",
      NA_character_
    ),
    LSTALVDT_D = if_else(!is.na(DTHDT_D) & !is.na(MAXALIVE) & MAXALIVE > DTHDT_D, DTHDT_D, MAXALIVE),
    LSTALVDT = fmt_date(LSTALVDT_D),
    DTHDT = fmt_date(DTHDT_D)
  ) |>
  select(STUDYID, USUBJID, LSTALVDT, DTHFL, DTHDT, DTHDTF)

dir.create(output_dir, showWarnings = FALSE, recursive = TRUE)
write_csv(adsl, file.path(output_dir, "adsl.csv"), na = "")
