# Reference solution for the yamaa benchmark sdtm-ex-infusion-duration (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ex_raw <- read_csv(
  "/app/input/ex_raw.csv",
  col_types = cols(.default = col_character())
)

# Whole minutes as ISO 8601 duration text, a day the largest part and every
# part that is zero left out; zero minutes is PT0S.
duration_text <- function(minutes) {
  vapply(minutes, function(m) {
    if (is.na(m)) return(NA_character_)
    if (m == 0L) return("PT0S")
    days <- m %/% 1440L
    hours <- (m %% 1440L) %/% 60L
    mins <- m %% 60L
    text <- "P"
    if (days > 0L) text <- paste0(text, days, "D")
    if (hours > 0L || mins > 0L) {
      text <- paste0(text, "T")
      if (hours > 0L) text <- paste0(text, hours, "H")
      if (mins > 0L) text <- paste0(text, mins, "M")
    }
    text
  }, character(1), USE.NAMES = FALSE)
}

ex <- ex_raw |>
  mutate(
    DOMAIN = "EX",
    EXDUR = duration_text(as.integer(EXDURMIN))
  ) |>
  select(DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXSTDTC, EXDUR)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ex, "/app/output/ex.csv", na = "")
