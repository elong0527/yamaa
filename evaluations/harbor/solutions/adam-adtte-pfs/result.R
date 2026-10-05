# Reference solution for the yamaa benchmark adam-adtte-pfs (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(
    RANDDT = col_date(), NTXSTDT = col_date(), .default = col_character()
  )
)
rs <- read_csv(
  "/app/input/rs.csv",
  col_types = cols(RSSEQ = col_integer(), RSDTC = col_date(), .default = col_character())
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(DSSEQ = col_integer(), DSDTC = col_date(), .default = col_character())
)

# The first adequate progression, the first death, and the last adequate
# assessment. Records sharing a date are ordered by sequence number.
progression <- rs |>
  filter(RSTESTCD %in% "OVRLRESP", RSSTRESC %in% "PD", ADEQFL %in% "Y", !is.na(RSDTC)) |>
  arrange(USUBJID, RSDTC, RSSEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, PDDT = RSDTC, PDSEQ = RSSEQ)

death <- ds |>
  filter(DSDECOD %in% "DEATH", !is.na(DSDTC)) |>
  arrange(USUBJID, DSDTC, DSSEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, DTHDT = DSDTC, DTHSEQ = DSSEQ)

last_assessment <- rs |>
  filter(RSTESTCD %in% "OVRLRESP", ADEQFL %in% "Y", !is.na(RSDTC)) |>
  arrange(USUBJID, desc(RSDTC), desc(RSSEQ)) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, CENSORDT = RSDTC, LASTSEQ = RSSEQ)

# The last adequate assessment dated on or before new-therapy start, for
# subjects who began one.
pre_therapy <- rs |>
  inner_join(select(adsl, USUBJID, NTXSTDT), by = "USUBJID") |>
  filter(
    RSTESTCD %in% "OVRLRESP", ADEQFL %in% "Y", !is.na(RSDTC),
    !is.na(NTXSTDT), RSDTC <= NTXSTDT
  ) |>
  arrange(USUBJID, desc(RSDTC), desc(RSSEQ)) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, PRECENSORDT = RSDTC, PRESEQ = RSSEQ)

adtte <- adsl |>
  left_join(progression, by = "USUBJID") |>
  left_join(death, by = "USUBJID") |>
  left_join(last_assessment, by = "USUBJID") |>
  left_join(pre_therapy, by = "USUBJID") |>
  mutate(
    PARAMCD = "PFS",
    PARAM = "Progression-Free Survival",
    STARTDT = RANDDT,
    # A progression or death dated after new-therapy start is not an event.
    PDDT = if_else(!is.na(NTXSTDT) & PDDT > NTXSTDT, as.Date(NA), PDDT),
    DTHDT = if_else(!is.na(NTXSTDT) & DTHDT > NTXSTDT, as.Date(NA), DTHDT),
    # The censoring assessment: the last adequate one overall, or the last
    # one dated on or before therapy start when therapy began.
    CENSORASSESSDT = if_else(is.na(NTXSTDT), CENSORDT, PRECENSORDT),
    CENSORSEQ = if_else(is.na(NTXSTDT), LASTSEQ, PRESEQ),
    # With no usable adequate assessment the subject is censored at
    # randomization, for a 1-day PFS.
    CENSORDT = coalesce(CENSORASSESSDT, STARTDT),
    EVENTDT = case_when(
      !is.na(PDDT) & !is.na(DTHDT) ~ pmin(PDDT, DTHDT),
      !is.na(PDDT) ~ PDDT,
      !is.na(DTHDT) ~ DTHDT
    ),
    # The event always wins, even after the last adequate assessment,
    # unless new anti-cancer therapy started first.
    CNSR = if_else(!is.na(EVENTDT), 0L, 1L),
    ADT = coalesce(EVENTDT, CENSORDT),
    AVAL = as.integer(ADT - STARTDT) + 1L,
    # A progression and a death on the same day count as progression.
    EVNTDESC = case_when(
      is.na(EVENTDT) ~ "CENSORED",
      !is.na(PDDT) & (is.na(DTHDT) | PDDT <= DTHDT) ~ "DISEASE PROGRESSION",
      .default = "DEATH"
    ),
    SRCDOM = case_when(
      EVNTDESC == "DEATH" ~ "DS",
      is.na(CENSORASSESSDT) ~ NA_character_,
      .default = "RS"
    ),
    SRCVAR = case_when(
      EVNTDESC == "DEATH" ~ "DSDTC",
      is.na(CENSORASSESSDT) ~ NA_character_,
      .default = "RSDTC"
    ),
    SRCSEQ = case_when(
      EVNTDESC == "DISEASE PROGRESSION" ~ PDSEQ,
      EVNTDESC == "DEATH" ~ DTHSEQ,
      is.na(CENSORASSESSDT) ~ NA_integer_,
      .default = CENSORSEQ
    )
  ) |>
  arrange(USUBJID) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
    SRCDOM, SRCVAR, SRCSEQ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtte, "/app/output/adtte.csv", na = "")
