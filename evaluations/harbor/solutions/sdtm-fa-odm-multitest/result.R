# Reference solution for the yamaa benchmark sdtm-fa-odm-multitest (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(purrr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
) |>
  mutate(Value = na_if(Value, ""))

forms <- odm |>
  filter(ItemGroupOID == "IG.FA") |>
  distinct(
    StudyOID, SubjectKey, StudyEventOID, StudyEventRepeatKey,
    ItemGroupRepeatKey
  )

one_form <- function(study, subj, event, rep, igrep) {
  sub <- odm |>
    filter(
      SubjectKey == subj,
      StudyEventOID == event,
      StudyEventRepeatKey == rep,
      ItemGroupOID == "IG.FA",
      ItemGroupRepeatKey == igrep
    )
  present <- sub$ItemOID
  get <- function(oid) {
    hit <- sub |> filter(ItemOID == oid)
    if (nrow(hit) == 0) return(list(present = FALSE, value = NA_character_))
    list(present = TRUE, value = hit$Value[[1]])
  }
  obj <- get("IT.FA.FAOBJ")
  if (!obj$present || is.na(obj$value)) return(tibble())
  fadtc <- get("IT.FA.FADTC")$value
  day <- dplyr::recode(event, DAY1 = 1L, DAY2 = 2L, .default = 99L)
  fatpt <- dplyr::recode(
    event, DAY1 = "END DAY 1", DAY2 = "END DAY 2", .default = NA_character_
  )
  specs <- list(
    list(oid = "IT.FA.OCCUR", code = "OCCUR", name = "Occurrence Indicator"),
    list(oid = "IT.FA.SEV", code = "SEV", name = "Severity/Intensity"),
    list(oid = "IT.FA.LDIAM", code = "LDIAM", name = "Longest Diameter")
  )
  imap_dfr(specs, function(spec, i) {
    g <- get(spec$oid)
    if (!g$present) return(tibble())
    blank <- is.na(g$value)
    tibble(
      DOMAIN = "FA",
      STUDYID = study,
      USUBJID = subj,
      DAY = day,
      REP = as.integer(igrep),
      ORDER = as.integer(i - 1L),
      FATESTCD = spec$code,
      FATEST = spec$name,
      FAOBJ = obj$value,
      FACAT = "REACTOGENICITY",
      FAORRES = g$value,
      FAORRESU = if_else(spec$oid == "IT.FA.LDIAM", "mm", NA_character_),
      FASTRESC = g$value,
      FASTAT = if_else(blank, "NOT DONE", NA_character_),
      FATPT = fatpt,
      FADTC = fadtc
    )
  })
}

fa <- pmap_dfr(
  list(
    forms$StudyOID, forms$SubjectKey, forms$StudyEventOID,
    forms$StudyEventRepeatKey, forms$ItemGroupRepeatKey
  ),
  one_form
) |>
  arrange(STUDYID, USUBJID, DAY, REP, ORDER) |>
  group_by(STUDYID, USUBJID) |>
  mutate(FASEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT,
    FAORRES, FAORRESU, FASTRESC, FASTAT, FATPT, FADTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(fa, "/app/output/fa.csv", na = "")
