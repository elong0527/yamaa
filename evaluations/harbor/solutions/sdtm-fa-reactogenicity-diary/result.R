# Reference solution for the yamaa benchmark sdtm-fa-reactogenicity-diary (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(purrr)

diary <- read_csv(
  "/app/input/diary.csv",
  col_types = cols(DIARYDAY = col_integer(), .default = col_character())
)

local_reactions <- c("PAIN", "REDNESS", "SWELLING")

one_row <- function(r) {
  fascat <- if (r$CATSRC == "LOCAL") {
    "ADMINISTRATION SITE"
  } else if (r$CATSRC == "SYSTEMIC") {
    "SYSTEMIC"
  } else if (r$REACTION %in% local_reactions) {
    "ADMINISTRATION SITE"
  } else {
    "SYSTEMIC"
  }
  base <- tibble(
    DOMAIN = "FA",
    STUDYID = r$STUDYID,
    USUBJID = r$USUBJID,
    DAY = r$DIARYDAY,
    REACTION = r$REACTION,
    FAOBJ = r$REACTION,
    FACAT = "REACTOGENICITY",
    FASCAT = fascat,
    FATPT = r$TPT,
    FADTC = r$DTC
  )
  clean <- function(x) {
    if (is.na(x) || x == "") NA_character_ else x
  }
  if (!is.na(r$COMPLETED) && r$COMPLETED != "Y") {
    return(base |> mutate(
      ORDER = 0L, FATESTCD = "OCCUR",
      FATEST = "Occurrence Indicator",
      FAORRES = NA_character_, FAORRESU = NA_character_,
      FASTRESC = NA_character_, FASTRESN = NA_real_,
      FASTRESU = NA_character_, FASTAT = "NOT DONE"
    ))
  }
  occur <- clean(r$OCCUR)
  sev <- clean(r$SEV)
  diam <- clean(r$DIAMETER)
  unit <- clean(r$DIAMUNIT)
  out <- base |> mutate(
    ORDER = 0L, FATESTCD = "OCCUR",
    FATEST = "Occurrence Indicator",
    FAORRES = occur, FAORRESU = NA_character_,
    FASTRESC = occur, FASTRESN = NA_real_,
    FASTRESU = NA_character_, FASTAT = NA_character_
  )
  out <- bind_rows(
    out,
    base |> mutate(
      ORDER = 1L, FATESTCD = "SEV",
      FATEST = "Severity/Intensity",
      FAORRES = sev, FAORRESU = NA_character_,
      FASTRESC = sev, FASTRESN = NA_real_,
      FASTRESU = NA_character_, FASTAT = NA_character_
    )
  )
  if (r$REACTION %in% c("REDNESS", "SWELLING") && !is.na(occur) && occur == "Y") {
    out <- bind_rows(
      out,
      base |> mutate(
        ORDER = 2L, FATESTCD = "LDIAM",
        FATEST = "Longest Diameter",
        FAORRES = diam, FAORRESU = unit,
        FASTRESC = diam,
        FASTRESN = suppressWarnings(as.numeric(diam)),
        FASTRESU = unit, FASTAT = NA_character_
      )
    )
  }
  out
}

fa <- diary |>
  split(seq_len(nrow(diary))) |>
  map_dfr(~ one_row(as.list(.x))) |>
  arrange(STUDYID, USUBJID, DAY, REACTION, ORDER) |>
  group_by(STUDYID, USUBJID) |>
  mutate(FASEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT,
    FASCAT, FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT,
    FATPT, FADTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(fa, "/app/output/fa.csv", na = "")
