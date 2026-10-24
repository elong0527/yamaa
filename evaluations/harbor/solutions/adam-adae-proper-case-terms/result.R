# Reference solution for the yamaa benchmark adam-adae-proper-case-terms (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

ae <- read.csv(
  "/app/input/ae.csv",
  colClasses = "character",
  na.strings = ""
)

# The first character, and each character that directly follows one of the
# delimiters, in upper case; every other letter in lower case. A term with no
# value is "Not Reported".
propcase <- function(terms, delimiters) {
  separators <- strsplit(delimiters, "")[[1]]
  vapply(terms, function(term) {
    if (is.na(term)) {
      return("Not Reported")
    }
    characters <- strsplit(term, "")[[1]]
    previous <- c("", head(characters, -1))
    raise <- seq_along(characters) == 1 | previous %in% separators
    paste(ifelse(raise, toupper(characters), tolower(characters)), collapse = "")
  }, character(1), USE.NAMES = FALSE)
}

adae <- data.frame(
  STUDYID = ae$STUDYID,
  USUBJID = ae$USUBJID,
  AESEQ = as.integer(ae$AESEQ),
  AETERM = ae$AETERM,
  # SAS PROPCASE's default delimiters, then the ones that replace them.
  AETERMPC = propcase(ae$AETERM, " /-(.\t"),
  AETERMPB = propcase(ae$AETERM, " /-(.,[")
)

dir.create("/app/output", showWarnings = FALSE)
write.csv(adae, "/app/output/adae.csv", row.names = FALSE, na = "")
