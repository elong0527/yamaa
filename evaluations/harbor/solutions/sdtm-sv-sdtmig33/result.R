visits <- read.csv("/app/input/visits.csv", stringsAsFactors = FALSE,
                   na.strings = c("", "NA"), check.names = FALSE)

# The source visit numbers already distinguish scheduled and intercurrent visits.
visitnum <- as.numeric(visits$VISITNO)
unplanned <- (!is.na(visitnum) & visitnum != floor(visitnum)) |
             grepl("UNSCHEDULED|UNPLANNED", visits$NAME, ignore.case = TRUE)
reason <- rep(NA_character_, nrow(visits))
reason[unplanned] <- visits$REASON[unplanned]

sv <- data.frame(
  DOMAIN = rep("SV", nrow(visits)),
  STUDYID = visits$STUDYID,
  USUBJID = visits$USUBJID,
  VISITNUM = visitnum,
  VISIT = visits$NAME,
  SVSTDTC = visits$START,
  SVENDTC = visits$END,
  SVUPDES = reason,
  stringsAsFactors = FALSE
)
sv <- sv[order(sv$STUDYID, sv$USUBJID, sv$VISITNUM), , drop = FALSE]
stopifnot(!anyDuplicated(sv[c("STUDYID", "USUBJID", "VISITNUM")]))
write.csv(sv, "/app/output/sv.csv", row.names = FALSE, na = "")
