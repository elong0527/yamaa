# Reference solution for the yamaa benchmark adam-adrs-response-prep (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adrs_raw.csv",
  col_types = cols(
    ASEQ = col_integer(),
    ADT = col_date(),
    RANDDY = col_integer(),
    .default = col_character()
  )
)

# The response category each record can support; stable and
# neither-complete-nor-progressive disease count only on or after day 42,
# and earlier ones, or ones with no day, fall back to not evaluable.
based <- raw |>
  mutate(
    BORCAT = case_when(
      AVALC %in% c("CR", "PR", "PD", "NE") ~ AVALC,
      AVALC %in% "SD" & !is.na(RANDDY) & RANDDY >= 42L ~ "SD",
      AVALC %in% "SD" ~ "NE",
      AVALC %in% "NON-CR/NON-PD" & !is.na(RANDDY) & RANDDY >= 42L ~ "NON-CR/NON-PD",
      AVALC %in% "NON-CR/NON-PD" ~ "NE",
      .default = NA_character_
    ),
    BORPRI = case_when(
      BORCAT == "CR" ~ 1L,
      BORCAT == "PR" ~ 2L,
      BORCAT == "SD" ~ 3L,
      BORCAT == "NON-CR/NON-PD" ~ 4L,
      BORCAT == "PD" ~ 5L,
      BORCAT == "NE" ~ 6L
    )
  )

# Usable records numbered from 1 in priority, date, then sequence order;
# records supporting no category are passed over without a gap.
order <- based |>
  filter(!is.na(BORCAT)) |>
  arrange(STUDYID, USUBJID, BORPRI, ADT, ASEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(BORSEQ = row_number()) |>
  ungroup() |>
  select(STUDYID, USUBJID, ASEQ, BORSEQ)

adrs <- based |>
  left_join(order, by = c("STUDYID", "USUBJID", "ASEQ")) |>
  select(STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ) |>
  arrange(STUDYID, USUBJID, ASEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adrs, "/app/output/adrs.csv", na = "")
