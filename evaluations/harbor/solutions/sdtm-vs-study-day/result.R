# Reference solution for the yamaa benchmark sdtm-vs-study-day (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(purrr)
library(readr)

study_day <- function(ref, coll) {
  if (is.na(ref) || is.na(coll)) {
    return(NA_integer_)
  }
  delta <- as.integer(coll - ref)
  if (delta >= 0L) delta + 1L else delta
}

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
se <- read_csv(
  "/app/input/se.csv",
  col_types = cols(.default = col_character())
)
tv <- read_csv(
  "/app/input/tv.csv",
  col_types = cols(.default = col_character())
)
vs_raw <- read_csv(
  "/app/input/vs_raw.csv",
  col_types = cols(VSSEQ = col_integer(), .default = col_character())
)

tv_map <- setNames(suppressWarnings(as.numeric(tv$VISITNUM)), tv$VISIT)
ref_map <- setNames(suppressWarnings(ymd(dm$RFSTDTC)), dm$USUBJID)

vs <- vs_raw |>
  mutate(
    coll = suppressWarnings(ymd(VSDTC)),
    ref = ref_map[USUBJID],
    VSDY = map2_int(ref, coll, study_day, .progress = FALSE),
    DOMAIN = "VS"
  )
vs$VISITNUM <- pmap_dbl(
  list(vs$USUBJID, vs$VISIT, vs$VSDTC),
  function(usubjid, visit, vsdtc) {
    if (visit %in% names(tv_map)) {
      return(tv_map[[visit]])
    }
    day <- suppressWarnings(ymd(vsdtc))
    if (is.na(day)) {
      return(NA_real_)
    }
    prior <- vs_raw |>
      filter(
        USUBJID == usubjid,
        VISIT %in% names(tv_map),
        !is.na(suppressWarnings(ymd(VSDTC))),
        suppressWarnings(ymd(VSDTC)) < day
      )
    if (nrow(prior) == 0) {
      return(NA_real_)
    }
    max(tv_map[prior$VISIT], na.rm = TRUE) + 0.01
  },
  .progress = FALSE
)
vs$EPOCH <- pmap_chr(
  list(vs$USUBJID, vs$VSDTC),
  function(usubjid, vsdtc) {
    day <- suppressWarnings(ymd(vsdtc))
    if (is.na(day)) {
      return(NA_character_)
    }
    elements <- se |> filter(USUBJID == usubjid)
    hits <- elements |>
      filter(
        suppressWarnings(ymd(SESTDTC)) <= day,
        day <= suppressWarnings(ymd(SEENDTC))
      ) |>
      arrange(suppressWarnings(ymd(SESTDTC)))
    if (nrow(hits) == 0) {
      return(NA_character_)
    }
    hits$EPOCH[nrow(hits)]
  },
  .progress = FALSE
)
vs <- vs |>
  arrange(USUBJID, VSSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT,
    VISITNUM, EPOCH, VSDY
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
