lab <- read.csv("/app/input/lab_transfer.csv", colClasses = "character",
                na.strings = character(), check.names = FALSE)
lab <- lab[order(lab$Protocol, lab$Subject, lab$Collected, lab$Sample), , drop = FALSE]
nominal_hours <- rep(NA_real_, nrow(lab))
nominal_hours[lab$Nominal == "PRE"] <- 0
planned <- grepl("^[0-9]+(\\.[0-9]+)?H$", lab$Nominal)
nominal_hours[planned] <- as.numeric(sub("H$", "", lab$Nominal[planned]))
seqnum <- integer(nrow(lab))
if (nrow(lab)) {
  group_start <- c(TRUE, lab$Protocol[-1] != lab$Protocol[-nrow(lab)] |
                          lab$Subject[-1] != lab$Subject[-nrow(lab)])
  seqnum <- sequence(tabulate(cumsum(group_start)))
}
pc <- data.frame(
  DOMAIN = rep("PC", nrow(lab)),
  STUDYID = lab$Protocol,
  USUBJID = lab$Subject,
  PCSEQ = seqnum,
  PCREFID = lab$Sample,
  PCTESTCD = rep("ZX201", nrow(lab)),
  PCTEST = rep("ZX-201", nrow(lab)),
  PCORRES = lab$Result,
  PCORRESU = rep("ng/mL", nrow(lab)),
  PCSTRESC = lab$Result,
  PCSTRESN = suppressWarnings(as.numeric(lab$Result)),
  PCSTRESU = rep("ng/mL", nrow(lab)),
  PCSPEC = rep("PLASMA", nrow(lab)),
  PCDTC = lab$Collected,
  PCTPT = lab$Nominal,
  PCTPTNUM = nominal_hours,
  stringsAsFactors = FALSE
)
write.csv(pc, "/app/output/pc.csv", row.names = FALSE, na = "")
stopifnot(nrow(pc) == nrow(lab), identical(pc$PCORRES, pc$PCSTRESC),
          all(is.na(pc$PCSTRESN[pc$PCORRES == "BLQ"])),
          all(is.na(pc$PCTPTNUM[pc$PCTPT == "UNSCHED"])))
