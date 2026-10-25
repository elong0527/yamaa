# Reference solution for the yamaa benchmark sdtm-suppmh-long-terms (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

mh <- read_csv("/app/input/mh_raw.csv", col_types = cols(.default = col_character()),
               na = character(), trim_ws = FALSE)

split_term <- function(x, width = 200) {
  pieces <- character()
  rest <- x
  while (nchar(rest) > width) {
    head <- substr(rest, 1, width + 1)
    sp <- gregexpr(" ", head, fixed = TRUE)[[1]]
    sp <- sp[sp > 1]
    if (length(sp) == 0) {
      # no break point: a single word longer than width
      pieces <- c(pieces, substr(rest, 1, width))
      rest <- substr(rest, width + 1, nchar(rest))
    } else {
      pos <- max(sp)
      pieces <- c(pieces, substr(rest, 1, pos - 1))
      rest <- substr(rest, pos + 1, nchar(rest))
    }
  }
  c(pieces, rest)
}

labels <- c(MHTERM1 = "Reported Term for the Medical History 1",
            MHTERM2 = "Reported Term for the Medical History 2")

rows <- lapply(seq_len(nrow(mh)), function(i) {
  p <- split_term(mh$MHTERM[i])
  if (length(p) < 2) return(NULL)
  extra <- p[-1]
  qnam <- paste0("MHTERM", seq_along(extra))
  tibble(
    STUDYID = mh$STUDYID[i],
    RDOMAIN = "MH",
    USUBJID = mh$USUBJID[i],
    IDVAR = "MHSEQ",
    IDVARVAL = as.character(as.integer(mh$MHSEQ[i])),
    QNAM = qnam,
    QLABEL = unname(labels[qnam]),
    QVAL = extra,
    QORIG = "Collected",
    QEVAL = NA_character_
  )
})

suppmh <- bind_rows(rows)
if (nrow(suppmh) == 0) {
  suppmh <- tibble(STUDYID = character(), RDOMAIN = character(), USUBJID = character(),
                   IDVAR = character(), IDVARVAL = character(), QNAM = character(),
                   QLABEL = character(), QVAL = character(), QORIG = character(),
                   QEVAL = character())
}
suppmh <- suppmh %>%
  arrange(STUDYID, USUBJID, as.integer(IDVARVAL), QNAM) %>%
  select(STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL)

write_csv(suppmh, "/app/output/suppmh.csv", na = "")
