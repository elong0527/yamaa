# Reference solution for the yamaa benchmark adam-adae-worst-severity (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adae_raw <- read_csv(
  "/app/input/adae_raw.csv",
  col_types = cols(AESEQ = col_integer(), ASTDT = col_date(), .default = col_character())
)

adae <- adae_raw |>
  mutate(
    AESEVN = case_when(
      AESEV == "MILD" ~ 1L,
      AESEV == "MODERATE" ~ 2L,
      AESEV == "SEVERE" ~ 3L
    )
  )

# The eligible event with the greatest rank per subject and term; ties break
# by earliest date, then lowest sequence. Only a treatment-emergent event
# with a graded severity is eligible.
best <- adae |>
  filter(TRTEMFL == "Y", !is.na(AESEVN)) |>
  arrange(USUBJID, AEDECOD, desc(AESEVN), ASTDT, AESEQ) |>
  distinct(USUBJID, AEDECOD, .keep_all = TRUE) |>
  select(USUBJID, AEDECOD, AESEQ) |>
  mutate(BEST = "Y")

adae <- adae |>
  left_join(best, by = c("USUBJID", "AEDECOD", "AESEQ")) |>
  mutate(AWSEVFL = BEST) |>
  select(
    STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, AESEV, TRTEMFL,
    AESEVN, AWSEVFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
