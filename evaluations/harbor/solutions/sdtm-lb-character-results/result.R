# Reference solution for the yamaa benchmark sdtm-lb-character-results (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(purrr)
library(stringr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
) |>
  mutate(Value = na_if(Value, ""))

form_order <- c(
  "FO.LB_PROT" = 0L, "FO.LB_CK" = 1L, "FO.LB_GLUC" = 2L,
  "FO.LB_KETON" = 3L, "FO.LB_CREAT" = 4L
)
form_test <- c(
  "FO.LB_PROT" = "PROT", "FO.LB_CK" = "CK", "FO.LB_GLUC" = "GLUC",
  "FO.LB_KETON" = "KETON", "FO.LB_CREAT" = "CREAT"
)
test_name <- c(
  PROT = "Protein", CK = "Creatine Kinase", GLUC = "Glucose",
  KETON = "Ketones", CREAT = "Creatinine"
)

standardize <- function(raw) {
  if (is.na(raw)) return(NA_character_)
  low <- str_to_lower(str_replace_all(str_trim(raw), " ", ""))
  if (low %in% c("negative", "neg")) return("NEGATIVE")
  if (low %in% c("trace", "tr")) return("TRACE")
  if (low %in% c("1+", "+1", "1plus")) return("1+")
  if (low %in% c("2+", "+2", "2plus")) return("2+")
  raw
}

forms <- odm |>
  distinct(StudyOID, SubjectKey, FormOID, FormRepeatKey) |>
  filter(FormOID %in% names(form_order))

one_form <- function(study, subj, formoid, formrep) {
  sub <- odm |>
    filter(
      SubjectKey == subj, FormOID == formoid, FormRepeatKey == formrep
    )
  get <- function(oid) {
    hit <- sub |> filter(ItemOID == oid)
    if (nrow(hit) == 0) return(NA_character_)
    hit$Value[[1]]
  }
  raw <- get("IT.LB.RESULT")
  if (is.na(raw)) return(tibble())
  code <- form_test[[formoid]]
  stresc <- standardize(raw)
  stresn <- suppressWarnings(as.numeric(raw))
  if (!is.na(stresc) && stresc %in% c("NEGATIVE", "TRACE", "1+", "2+")) {
    stresn <- NA_real_
  }
  if (str_starts(str_trim(raw), "<") || str_starts(str_trim(raw), ">")) {
    stresn <- NA_real_
  }
  nrind <- if (str_starts(str_trim(raw), "<")) {
    "LOW"
  } else if (str_starts(str_trim(raw), ">")) {
    "HIGH"
  } else {
    NA_character_
  }
  unit <- get("IT.LB.LBORRESU")
  tibble(
    DOMAIN = "LB",
    STUDYID = study,
    USUBJID = subj,
    ORDER = form_order[[formoid]],
    REP = as.integer(formrep),
    LBTESTCD = code,
    LBTEST = test_name[[code]],
    LBORRES = raw,
    LBORRESU = unit,
    LBSTRESC = stresc,
    LBSTRESN = stresn,
    LBSTRESU = unit,
    LBNRIND = nrind,
    LBDTC = get("IT.LB.LBDTC")
  )
}

lb <- pmap_dfr(
  list(forms$StudyOID, forms$SubjectKey, forms$FormOID, forms$FormRepeatKey),
  one_form
) |>
  arrange(STUDYID, USUBJID, ORDER, REP) |>
  group_by(STUDYID, USUBJID) |>
  mutate(LBSEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES,
    LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBNRIND, LBDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
