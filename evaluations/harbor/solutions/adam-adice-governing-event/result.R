events <- read.csv("/app/input/ice_raw.csv", stringsAsFactors = FALSE)
priority <- c(ICE04 = 1L, ICE05 = 1L, ICE01 = 2L, ICE02 = 3L, ICE03 = 4L)
events$ICEPRIO <- unname(priority[events$ICETYPE])
events <- events[order(events$STUDYID, events$USUBJID,
                       events$ICEPRIO, events$ICEDTC, events$ICESEQ), ]
groups <- split(events, interaction(events$STUDYID, events$USUBJID,
                                   drop = TRUE))
result <- do.call(rbind, lapply(groups, function(group) {
  group$ICERANK <- seq_len(nrow(group))
  group$GOVFL <- ifelse(group$ICERANK == 1L, "Y", NA_character_)
  group$SUPERFL <- ifelse(group$ICERANK > 1L, "Y", NA_character_)
  group$GOVTYPE <- group$ICETYPE[1]
  group$GOVDT <- group$ICEDTC[1]
  group$GOVSEQ <- group$ICESEQ[1]
  group
}))
names(result)[names(result) == "ICESEQ"] <- "ASEQ"
names(result)[names(result) == "ICETYPE"] <- "ADECOD1"
names(result)[names(result) == "ICEDTC"] <- "ASTDT"
columns <- c("STUDYID", "USUBJID", "ASEQ", "ADECOD1", "ASTDT", "ICEPRIO",
             "ICERANK", "GOVFL", "SUPERFL", "GOVTYPE", "GOVDT", "GOVSEQ")
result <- result[order(result$STUDYID, result$USUBJID, result$ASEQ), columns]
write.csv(result, "/app/output/adice.csv", row.names = FALSE, na = "")
