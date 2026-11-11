# Reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(purrr)
library(readr)

parse_moment <- function(x) {
  if (is.na(x) || x == "") {
    return(as.POSIXct(NA, tz = "UTC"))
  }
  # A partial date such as YYYY-MM has no day, so it cannot be placed.
  if (nchar(x) == 7 && substr(x, 5, 5) == "-") {
    return(as.POSIXct(NA, tz = "UTC"))
  }
  if (nchar(x) == 4 && grepl("^[0-9]{4}$", x)) {
    return(as.POSIXct(NA, tz = "UTC"))
  }
  # A date alone has no time and is compared by date instead.
  if (!grepl("T", x, fixed = TRUE)) {
    as.POSIXct(NA, tz = "UTC")
  } else {
    # Element bounds and collections use ISO datetimes without seconds.
    out <- suppressWarnings(ymd_hm(x, tz = "UTC"))
    if (is.na(out)) {
      out <- suppressWarnings(ymd_hms(x, tz = "UTC"))
    }
    out
  }
}

se <- read_csv(
  "/app/input/se.csv",
  col_types = cols(.default = col_character())
)
vs_raw <- read_csv(
  "/app/input/vs_raw.csv",
  col_types = cols(VSSEQ = col_integer(), .default = col_character())
)

se <- se |>
  mutate(
    start = vapply(SESTDTC, function(x) as.numeric(parse_moment(x)), numeric(1)),
    end = vapply(SEENDTC, function(x) as.numeric(parse_moment(x)), numeric(1))
  )

vs_raw <- vs_raw |>
  mutate(
    moment = vapply(VSDTC, function(x) as.numeric(parse_moment(x)), numeric(1)),
    day = if_else(
      grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}$", coalesce(VSDTC, "")),
      suppressWarnings(as.numeric(ymd(VSDTC))),
      NA_real_
    )
  )

# The calendar dates of each element's start and end, in days.
se <- se |>
  mutate(
    start_day = as.numeric(as.Date(as.POSIXct(start, origin = "1970-01-01", tz = "UTC"))),
    end_day = as.numeric(as.Date(as.POSIXct(end, origin = "1970-01-01", tz = "UTC")))
  )

vs <- vs_raw |>
  mutate(
    DOMAIN = "VS",
    EPOCH = pmap_chr(
      list(USUBJID, moment, day),
      function(usubjid, coll, coll_day) {
        # A date-time is placed by moment, a date alone by calendar date;
        # when two elements match, the later one wins.
        if (!is.na(coll)) {
          hits <- se |> filter(USUBJID == usubjid, start <= coll, coll <= end)
        } else if (!is.na(coll_day)) {
          hits <- se |>
            filter(USUBJID == usubjid, start_day <= coll_day, coll_day <= end_day)
        } else {
          return(NA_character_)
        }
        hits <- hits |> arrange(start)
        if (nrow(hits) == 0) {
          return(NA_character_)
        }
        hits$EPOCH[nrow(hits)]
      },
      .progress = FALSE
    )
  ) |>
  arrange(USUBJID, VSSEQ) |>
  select(DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH)

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
