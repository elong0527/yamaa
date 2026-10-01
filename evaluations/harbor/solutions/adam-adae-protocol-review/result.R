# Reference solution for the yamaa benchmark adam-adae-protocol-review (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), SCORE = col_double(), .default = col_character())
)

adae <- ae |>
  mutate(
    ASTDT = if_else(is.na(AESTDTC) | AESTDTC == "", NA_character_, AESTDTC),
    # The calendar date of the collected start datetime.
    ASTDT2 = if_else(
      is.na(AESTDTM) | AESTDTM == "",
      NA_character_,
      str_extract(AESTDTM, "^\\d{4}-\\d{2}-\\d{2}")
    ),
    # The review window: the collected start date falls in January 2025,
    # or the collected start datetime falls at or after 09:30 on
    # 1 February 2025.
    in_window = str_starts(coalesce(ASTDT, ""), "2025-01") |
      (coalesce(AESTDTM, "") >= "2025-02-01T09:30" & !is.na(AESTDTM) & AESTDTM != ""),
    # The reported term begins with the literal text INF_.
    term_ok = str_starts(coalesce(AETERM, ""), "INF_"),
    score_ok = !is.na(SCORE) & SCORE >= -1.5,
    REVIEWFL = if_else(in_window & term_ok & score_ok, "Y", "N")
  ) |>
  select(STUDYID, USUBJID, AESEQ, ASTDT, ASTDT2, REVIEWFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
