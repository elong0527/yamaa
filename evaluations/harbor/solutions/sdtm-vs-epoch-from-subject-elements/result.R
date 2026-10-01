# Reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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
  if (!grepl("T", x, fixed = TRUE)) {
    suppressWarnings(ymd_hms(paste0(x, "T00:00:00"), tz = "UTC"))
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
  mutate(moment = vapply(VSDTC, function(x) as.numeric(parse_moment(x)), numeric(1)))

vs <- vs_raw |>
  mutate(
    DOMAIN = "VS",
    EPOCH = pmap_chr(
      list(USUBJID, moment),
      function(usubjid, coll) {
        if (is.na(coll)) {
          return(NA_character_)
        }
        hits <- se |>
          filter(USUBJID == usubjid, start <= coll, coll <= end) |>
          arrange(start)
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
