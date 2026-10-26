# Reference solution for the yamaa benchmark adam-adae-phase-emergence (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

# Derive ADAE with phase-specific treatment-emergent flags from ADSL and AE.
suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

input_dir <- "/app/input"
output_dir <- "/app/output"
dir.create(output_dir, showWarnings = FALSE, recursive = TRUE)

adsl <- read_csv(file.path(input_dir, "adsl.csv"),
                 col_types = cols(.default = col_character()),
                 na = c("", "NA"))
ae <- read_csv(file.path(input_dir, "ae.csv"),
               col_types = cols(.default = col_character()),
               na = c("", "NA"))

to_date <- function(x) as.Date(x, format = "%Y-%m-%d")

adsl <- adsl %>%
  mutate(across(matches("^TR0[123][SE]DT$"), to_date))

# Determine the phase (1, 2, 3) an event started in, or NA when none.
phase_of_event <- function(astdt, aeser, s1, e1, s2, e2, s3, e3, eotstt) {
  if (is.na(astdt)) return(NA_integer_)
  starts <- c(s1, s2, s3)
  ends <- c(e1, e2, e3)
  entered <- which(!is.na(starts))
  if (length(entered) == 0) return(NA_integer_)
  if (astdt < starts[entered[1]]) return(NA_integer_)
  for (i in seq_along(entered)) {
    k <- entered[i]
    if (astdt < starts[k]) next
    if (i < length(entered)) {
      phase_end <- starts[entered[i + 1]] - 1
      if (astdt <= phase_end) return(k)
    } else {
      if (!is.na(eotstt) && eotstt == "ONGOING") return(k)
      if (is.na(ends[k])) return(NA_integer_)
      window <- if (!is.na(aeser) && aeser == "Y") 90 else if (!is.na(aeser) && aeser == "N") 30 else NA
      if (is.na(window)) return(NA_integer_)
      if (astdt <= ends[k] + window) return(k)
    }
  }
  NA_integer_
}

adae <- ae %>%
  left_join(adsl %>% select(-STUDYID), by = "USUBJID") %>%
  mutate(
    ASTDT = to_date(AESTDTC),
    AESEQ = as.integer(AESEQ)
  )

adae$PHASE <- vapply(
  seq_len(nrow(adae)),
  function(r) {
    with(adae[r, ], phase_of_event(ASTDT, AESER, TR01SDT, TR01EDT, TR02SDT,
                                   TR02EDT, TR03SDT, TR03EDT, EOTSTT))
  },
  integer(1)
)

adae <- adae %>%
  mutate(
    TREM01FL = if_else(!is.na(PHASE) & PHASE == 1L, "Y", NA_character_),
    TREM02FL = if_else(!is.na(PHASE) & PHASE == 2L, "Y", NA_character_),
    TREM03FL = if_else(!is.na(PHASE) & PHASE == 3L, "Y", NA_character_),
    TRTEMFL = if_else(!is.na(PHASE), "Y", NA_character_)
  ) %>%
  arrange(STUDYID, USUBJID, AESEQ) %>%
  select(STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDT,
         TREM01FL, TREM02FL, TREM03FL, TRTEMFL)

write_csv(adae %>% mutate(ASTDT = format(ASTDT, "%Y-%m-%d")),
          file.path(output_dir, "adae.csv"), na = "")
