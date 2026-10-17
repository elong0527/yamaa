subjects <- read.csv("/app/input/adsl.csv", stringsAsFactors = FALSE)
events <- read.csv("/app/input/adae.csv", stringsAsFactors = FALSE)
subjects <- subjects[subjects$SAFFL == "Y", ]
events <- events[events$TRTEMFL == "Y", ]
subject_key <- paste(subjects$STUDYID, subjects$USUBJID, sep = "\r")
event_key <- paste(events$STUDYID, events$USUBJID, sep = "\r")
subjects$EVENTN <- as.integer(subject_key %in% event_key)
groups <- split(subjects, interaction(subjects$STUDYID, subjects$TRT01PN,
                                      subjects$TRT01P, drop = TRUE))
result <- do.call(rbind, lapply(groups, function(group) {
  count <- sum(group$EVENTN)
  data.frame(STUDYID = group$STUDYID[1], ANALYSISID = "AEINC",
    ANALYSISSETID = "SAFETY", DATASET = "ADAE", VARIABLE = "USUBJID",
    ANALYSISSET = "SAFFL = 'Y'", DATASUBSET = "TRTEMFL = 'Y'",
    GROUPINGID = "TRT01P", GROUPORDER = group$TRT01PN[1],
    GROUPLEVEL = group$TRT01P[1], METHODID = "AEINCIDENCE",
    OPERATIONID = c("COUNT", "PERCENT"),
    RESULT = c(count, 100 * count / nrow(group)))
}))
result <- result[order(result$STUDYID, result$GROUPORDER,
                       result$OPERATIONID), ]
write.csv(result, "/app/output/results.csv", row.names = FALSE, na = "")
