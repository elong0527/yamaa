# Reference solution for the yamaa benchmark adam-adaecm-related-pairs (R track).
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

events <- relrec[relrec$RDOMAIN == "AE" & relrec$IDVAR == "AESEQ",
                 c("STUDYID", "USUBJID", "RELID", "IDVARVAL")]
names(events)[4] <- "AESEQ"
medications <- relrec[relrec$RDOMAIN == "CM" & relrec$IDVAR == "CMSEQ",
                      c("STUDYID", "USUBJID", "RELID", "IDVARVAL")]
names(medications)[4] <- "CMSEQ"

# Every event of a relationship with every medication of the same subject's
# same relationship.
pairs <- merge(events, medications, by = c("STUDYID", "USUBJID", "RELID"))
pairs$AESEQ <- as.integer(pairs$AESEQ)
pairs$CMSEQ <- as.integer(pairs$CMSEQ)

pairs$AETERM <- ae$AETERM[match(
  paste(pairs$STUDYID, pairs$USUBJID, pairs$AESEQ, sep = "\r"),
  paste(ae$STUDYID, ae$USUBJID, as.integer(ae$AESEQ), sep = "\r")
)]
pairs$CMTRT <- cm$CMTRT[match(
  paste(pairs$STUDYID, pairs$USUBJID, pairs$CMSEQ, sep = "\r"),
  paste(cm$STUDYID, cm$USUBJID, as.integer(cm$CMSEQ), sep = "\r")
)]

adaecm <- pairs[order(pairs$STUDYID, pairs$USUBJID, pairs$AESEQ, pairs$CMSEQ,
                      method = "radix"),
                c("STUDYID", "USUBJID", "AESEQ", "AETERM", "CMSEQ", "CMTRT",
                  "RELID")]

dir.create("/app/output", showWarnings = FALSE, recursive = TRUE)
write.csv(adaecm, "/app/output/adaecm.csv", row.names = FALSE, na = "")
