# Reference solution for the yamaa benchmark sdtm-co-long-comments (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

# Build the SDTM CO (Comments) domain from CO_RAW.
# One record per subject per collected comment number; blank comments dropped.
# Comment text is split at fixed 200-character positions into COVAL,
# COVAL1 and COVAL2 (characters, not bytes; no word-boundary adjustment).

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

in_path  <- "/app/input/co_raw.csv"
out_path <- "/app/output/co.csv"

raw <- read_csv(
  in_path,
  col_types = cols(.default = col_character()),
  na = character(),          # keep text exactly as collected
  trim_ws = FALSE,
  locale = locale(encoding = "UTF-8")
)

piece <- function(x, start, stop) {
  n <- nchar(x, type = "chars")
  out <- substr(x, start, stop)
  out[is.na(x) | n < start] <- NA_character_
  out
}

co <- raw %>%
  mutate(COMMENT = ifelse(is.na(COMMENT), "", COMMENT)) %>%
  filter(trimws(COMMENT) != "") %>%
  transmute(
    DOMAIN  = "CO",
    STUDYID = STUDYID,
    USUBJID = USUBJID,
    COSEQ   = as.integer(COSEQ),
    COVAL   = piece(COMMENT, 1L, 200L),
    COVAL1  = piece(COMMENT, 201L, 400L),
    COVAL2  = piece(COMMENT, 401L, 600L)
  ) %>%
  arrange(STUDYID, USUBJID, COSEQ)

write_csv(co, out_path, na = "")
