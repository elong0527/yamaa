# Create the SDTM Demographics domain from the supplied subject data.
source_path <- file.path("/app/input", "dm_raw.csv")
output_path <- file.path("/app/output", "dm.csv")

DM_RAW <- read.csv(source_path, colClasses = "character",
                   stringsAsFactors = FALSE, na.strings = "",
                   check.names = FALSE)
required <- c("STUDYID", "USUBJID", "SUBJID", "RFSTDTC", "SITEID",
              "AGE", "SEX", "RACE", "COUNTRY")
stopifnot(all(required %in% names(DM_RAW)),
          !anyNA(DM_RAW$USUBJID), !anyDuplicated(DM_RAW$USUBJID))

DM <- data.frame(
  DOMAIN = rep("DM", nrow(DM_RAW)),
  STUDYID = DM_RAW$STUDYID,
  USUBJID = DM_RAW$USUBJID,
  SUBJID = DM_RAW$SUBJID,
  RFSTDTC = DM_RAW$RFSTDTC,
  SITEID = DM_RAW$SITEID,
  AGE = as.numeric(DM_RAW$AGE),
  AGEU = rep("YEARS", nrow(DM_RAW)),
  SEX = DM_RAW$SEX,
  RACE = DM_RAW$RACE,
  COUNTRY = DM_RAW$COUNTRY,
  stringsAsFactors = FALSE
)

dir.create("/app/output", showWarnings = FALSE)
write.csv(DM, output_path, row.names = FALSE, na = "")
