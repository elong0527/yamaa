# Reference solution for the yamaa benchmark sdtm-ae-meddra-hierarchy (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae_raw <- read_csv(
  "/app/input/ae_raw.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
meddra <- read_csv(
  "/app/input/meddra_synthetic.csv",
  col_types = cols(.default = col_character())
)

# Every hierarchy field comes from the primary path for the coder-assigned
# code; the extract marks it with PRIMARY_SOC Y. An unknown or absent code
# leaves the hierarchy empty, but the assigned code stays in AELLTCD.
primary <- meddra |> filter(PRIMARY_SOC == "Y")

ae <- ae_raw |>
  left_join(primary, by = c("AELLTCD" = "LLTCD")) |>
  mutate(
    DOMAIN = "AE",
    AELLT = LLTNAME,
    AEDECOD = PTNAME,
    AEPTCD = PTCD,
    AEHLT = HLTNAME,
    AEHLTCD = HLTCD,
    AEHLGT = HLGTNAME,
    AEHLGTCD = HLGTCD,
    AEBODSYS = SOCNAME,
    AEBODSCD = SOCCD,
    AESOC = SOCNAME,
    AESOCCD = SOCCD
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AELLT, AELLTCD,
    AEDECOD, AEPTCD, AEHLT, AEHLTCD, AEHLGT, AEHLGTCD,
    AEBODSYS, AEBODSCD, AESOC, AESOCCD
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ae, "/app/output/ae.csv", na = "")
