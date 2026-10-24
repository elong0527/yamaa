# Reference solution for the yamaa benchmark adam-adcm-free-text-start-date (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)

months <- c("JAN", "FEB", "MAR", "APR", "MAY", "JUN",
            "JUL", "AUG", "SEP", "OCT", "NOV", "DEC")

# The month a three-letter abbreviation names, in any letter case.
month_of <- function(abbreviation) match(toupper(abbreviation), months)

# A date written as ISO 8601 text; an impossible date is an error.
iso_date <- function(year, month, day) {
  value <- as.Date(sprintf("%04d-%02d-%02d", as.integer(year), as.integer(month),
                           as.integer(day)), format = "%Y-%m-%d")
  if (is.na(value)) stop("not a date: ", year, "-", month, "-", day)
  format(value, "%Y-%m-%d")
}

# The start as ISO 8601 text at the precision collected.
read_start <- function(text) {
  if (is.na(text) || text == "") return(NA_character_)
  if (grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}$", text)) {
    return(iso_date(substr(text, 1, 4), substr(text, 6, 7), substr(text, 9, 10)))
  }
  if (grepl("^[0-9]{2}[A-Za-z]{3}[0-9]{4}$", text) && !is.na(month_of(substr(text, 3, 5)))) {
    return(iso_date(substr(text, 6, 9), month_of(substr(text, 3, 5)), substr(text, 1, 2)))
  }
  if (grepl("^[0-9]{2}/[0-9]{2}/[0-9]{4}$", text)) {
    # The month comes first.
    return(iso_date(substr(text, 7, 10), substr(text, 1, 2), substr(text, 4, 5)))
  }
  if (grepl("^[0-9]{8}$", text)) {
    return(iso_date(substr(text, 1, 4), substr(text, 5, 6), substr(text, 7, 8)))
  }
  if (grepl("^[A-Za-z]{3}[0-9]{4}$", text) && !is.na(month_of(substr(text, 1, 3)))) {
    # A month alone.
    return(sprintf("%s-%02d", substr(text, 4, 7), month_of(substr(text, 1, 3))))
  }
  stop("CMSTDAT is in none of the five spellings: ", text)
}

adcm <- cm |>
  mutate(
    ASTDTC = vapply(CMSTDAT, read_start, character(1), USE.NAMES = FALSE),
    # A month alone starts on its first day.
    ASTDT = if_else(nchar(ASTDTC) == 7, paste0(ASTDTC, "-01"), ASTDTC)
  ) |>
  select(STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDAT, ASTDTC, ASTDT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adcm, "/app/output/adcm.csv", na = "")
