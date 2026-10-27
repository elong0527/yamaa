source_data <- read.csv("/app/input/morphology.csv", stringsAsFactors = FALSE,
                        colClasses = "character", check.names = FALSE)
required <- c("STUDYID", "USUBJID", "SEQ", "RESULT", "DATE")
stopifnot(all(required %in% names(source_data)))
standard_result <- rep(NA_character_, nrow(source_data))
standard_result[which(source_data$RESULT == "normal")] <- "NORMAL"
standard_result[which(source_data$RESULT == "abnormal")] <- "ABNORMAL"
mo <- data.frame(
  DOMAIN = rep("MO", nrow(source_data)),
  STUDYID = source_data$STUDYID,
  USUBJID = source_data$USUBJID,
  MOSEQ = as.integer(source_data$SEQ),
  MOTESTCD = rep("CELLTYPE", nrow(source_data)),
  MOTEST = rep("Cell Type", nrow(source_data)),
  MOORRES = source_data$RESULT,
  MOSTRESC = standard_result,
  MODTC = source_data$DATE,
  stringsAsFactors = FALSE
)
dir.create("/app/output", showWarnings = FALSE)
write.csv(mo, "/app/output/mo.csv", row.names = FALSE, na = "")
