lab <- read.csv("/app/input/lab_transfer.csv", colClasses = "character", check.names = FALSE,
                stringsAsFactors = FALSE, na.strings = NULL)
collection_time <- as.POSIXct(lab$Collected, format = "%Y-%m-%dT%H:%M:%OS", tz = "UTC")
lab <- lab[order(lab$Protocol, lab$Subject, collection_time, lab$Sample), , drop = FALSE]
seq_group <- interaction(lab$Protocol, lab$Subject, drop = TRUE)
pcseq <- ave(seq_len(nrow(lab)), seq_group, FUN = seq_along)
planned_hours <- rep(NA_real_, nrow(lab))
planned_hours[lab$Nominal == "PRE"] <- 0
postdose <- grepl("^[0-9]+(\\.[0-9]+)?H$", lab$Nominal)
planned_hours[postdose] <- as.numeric(sub("H$", "", lab$Nominal[postdose]))
pc <- data.frame(
  DOMAIN = "PC",
  STUDYID = lab$Protocol,
  USUBJID = lab$Subject,
  PCSEQ = as.integer(pcseq),
  PCREFID = lab$Sample,
  PCTESTCD = "ZX201",
  PCTEST = "ZX-201",
  PCORRES = lab$Result,
  PCORRESU = lab$Unit,
  PCSTRESC = lab$Result,
  PCSTRESN = suppressWarnings(as.numeric(lab$Result)),
  PCSTRESU = lab$Unit,
  PCSPEC = "PLASMA",
  PCDTC = lab$Collected,
  PCTPT = lab$Nominal,
  PCTPTNUM = planned_hours,
  stringsAsFactors = FALSE,
  check.names = FALSE
)
stopifnot(nrow(pc) == nrow(lab), identical(pc$PCORRES, pc$PCSTRESC), identical(pc$PCORRESU, pc$PCSTRESU))
write.csv(pc, "/app/output/pc.csv", row.names = FALSE, na = "")
