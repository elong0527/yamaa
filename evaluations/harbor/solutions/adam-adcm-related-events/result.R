# Reference solution for the yamaa benchmark adam-adcm-related-events (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

read_text <- function(name) {
  read.csv(
    file.path("/app/input", name),
    colClasses = "character",
    na.strings = character(0),
    check.names = FALSE
  )
}

ae <- read_text("ae.csv")
cm <- read_text("cm.csv")
relrec <- read_text("relrec.csv")

# Each adverse event a relationship names, with the event's reported term.
events <- relrec[relrec$RDOMAIN == "AE" & relrec$IDVAR == "AESEQ", ]
relae <- data.frame(
  STUDYID = events$STUDYID,
  USUBJID = events$USUBJID,
  RELID = events$RELID,
  AESEQ = as.integer(events$IDVARVAL),
  stringsAsFactors = FALSE
)
term_key <- paste(ae$STUDYID, ae$USUBJID, as.integer(ae$AESEQ), sep = "\r")
relae$AETERM <- ae$AETERM[match(
  paste(relae$STUDYID, relae$USUBJID, relae$AESEQ, sep = "\r"),
  term_key
)]
relae <- relae[order(relae$STUDYID, relae$USUBJID, relae$RELID, relae$AESEQ,
                     method = "radix"), ]

# The relationship each medication belongs to.
links <- relrec[relrec$RDOMAIN == "CM" & relrec$IDVAR == "CMSEQ", ]
relid <- links$RELID[match(
  paste(cm$STUDYID, cm$USUBJID, cm$CMSEQ, sep = "\r"),
  paste(links$STUDYID, links$USUBJID, links$IDVARVAL, sep = "\r")
)]

aeseqs <- character(nrow(cm))
aeterms <- character(nrow(cm))
aetermn <- integer(nrow(cm))
for (i in seq_len(nrow(cm))) {
  related <- relae[
    !is.na(relid[i]) & relae$STUDYID == cm$STUDYID[i] &
      relae$USUBJID == cm$USUBJID[i] & relae$RELID == relid[i],
  ]
  if (nrow(related) == 0) {
    aeseqs[i] <- NA_character_
    aeterms[i] <- NA_character_
    aetermn[i] <- NA_integer_
  } else {
    names <- sort(unique(related$AETERM), method = "radix")
    aeseqs[i] <- paste(sort(related$AESEQ), collapse = ", ")
    aeterms[i] <- paste(names, collapse = ", ")
    aetermn[i] <- length(names)
  }
}

adcm <- data.frame(
  STUDYID = cm$STUDYID,
  USUBJID = cm$USUBJID,
  CMSEQ = as.integer(cm$CMSEQ),
  CMTRT = cm$CMTRT,
  AESEQS = aeseqs,
  AETERMS = aeterms,
  AETERMN = aetermn,
  stringsAsFactors = FALSE
)
adcm <- adcm[order(adcm$STUDYID, adcm$USUBJID, adcm$CMSEQ, method = "radix"), ]

dir.create("/app/output", showWarnings = FALSE, recursive = TRUE)
write.csv(relae, "/app/output/relae.csv", row.names = FALSE, na = "")
write.csv(adcm, "/app/output/adcm.csv", row.names = FALSE, na = "")
