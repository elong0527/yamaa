# Independent installed R Define witnesses. The runtime has no Python executable.
library(yamaa)
fixtures <- system.file("define-fixtures", package = "yamaa", mustWork = TRUE)
artifacts <- get(".define_artifacts", envir = asNamespace("yamaa"))
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
fixture <- function(name, published = FALSE) {
  root <- tempfile("define-"); dir.create(root)
  stopifnot(file.copy(file.path(fixtures, name, c("spec.yaml", "environment.yaml")), root))
  if (name == "sdtm-lb-metadata") stopifnot(file.copy(file.path(fixtures, name, "submission-spec.yaml"), root))
  if (published) {
    dataset <- if (name == "sdtm-lb-metadata") "lb" else "dm"
    stopifnot(file.copy(file.path(fixtures, name, "expected", paste0(dataset, ".csv")), file.path(root, paste0(dataset, ".csv"))))
  }
  root
}
check_issues <- function(result, empty = TRUE) {
  frame <- result$issues
  stopifnot(is.data.frame(frame), identical(names(frame), c("phase", "condition", "requirement", "spec_paths", "context")),
    all(vapply(frame[c("phase", "condition", "requirement", "context")], is.character, logical(1))), is.list(frame$spec_paths))
  if (empty) stopifnot(nrow(frame) == 0L)
}
refused <- function(result) tryCatch({ result$save(); FALSE }, yamaa_define_error = function(e) {
  stopifnot(identical(e$issues, result$issues)); TRUE
})
original_path <- Sys.getenv("PATH")
native_path <- tempfile("define-runtime-"); dir.create(native_path)
rm_path <- Sys.which("rm")
if (nzchar(rm_path)) invisible(file.symlink(rm_path, file.path(native_path, "rm")))
Sys.setenv(PATH = native_path)
stopifnot(!nzchar(Sys.which("python")), !nzchar(Sys.which("python3")))

root <- fixture("sdtm-dm-metadata", TRUE)
result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
check_issues(result)
held <- artifacts(result)
stopifnot(length(held) == 2L, identical(vapply(held, `[[`, character(1), "path"), c("dm.json", "define.xml")),
  !dir.exists(file.path(root, "input")), !file.exists(file.path(root, "dm.json")), !file.exists(file.path(root, "define.xml")))
for (artifact in held) stopifnot(identical(artifact$bytes, rawfile(file.path(fixtures, "sdtm-dm-metadata/expected", artifact$path))))
unlink(file.path(root, c("spec.yaml", "environment.yaml")))
writeLines("changed after preparation", file.path(root, "dm.csv"))
stopifnot(isTRUE(result$save()), identical(result$save(), FALSE))
for (artifact in held) stopifnot(identical(rawfile(file.path(root, artifact$path)), artifact$bytes))
unlink(root, recursive = TRUE)

# The original LB spec and expected CSV remain immutable. The separate metadata
# completion explicitly supplies SDTM-valid sources and complete column facts.
root <- fixture("sdtm-lb-metadata", TRUE)
result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
check_issues(result); held <- artifacts(result)
stopifnot(length(held) == 2L, identical(vapply(held, `[[`, character(1), "path"), c("lb.json", "define.xml")))
xml <- rawToChar(held[[2L]]$bytes)
count <- function(pattern, text) sum(gregexpr(pattern, text, fixed = TRUE)[[1L]] > 0L)
stopifnot(count("<def:ValueListDef ", xml) == 1L, count("<def:ValueListRef ", xml) == 1L,
  count("<def:WhereClauseDef ", xml) == 2L, count("<ItemDef ", xml) == 13L,
  grepl('CreationDateTime="2026-09-11T00:00:00"', xml, fixed = TRUE),
  grepl('SourceSystemVersion="1.0"', xml, fixed = TRUE),
  grepl('<def:ValueListRef ValueListOID="VL.LB.LBORRESU"/>', xml, fixed = TRUE))
for (case in list(list(i = 1L, code = "GLUC", source = "Vendor", list = "CL_GLUCOSE"),
  list(i = 2L, code = "CREAT", source = "Sponsor", list = "CL_CREATININE"))) {
  identity <- paste0("LB.LBORRESU.V", case$i)
  item_start <- regexpr(paste0('<ItemDef OID="IT.', identity, '"'), xml, fixed = TRUE)[[1L]]
  stopifnot(item_start > 0L)
  item <- substring(xml, item_start)
  item <- substring(item, 1L, regexpr("</ItemDef>", item, fixed = TRUE)[[1L]])
  stopifnot(grepl('Name="LBORRESU" DataType="text" Length="8"', item, fixed = TRUE),
    grepl(paste0('CodeListOID="CL.', case$list, '"'), item, fixed = TRUE),
    grepl(paste0('<def:Origin Type="Assigned" Source="', case$source, '"/>'), item, fixed = TRUE),
    grepl(paste0('<def:WhereClauseRef WhereClauseOID="WC.', identity, '"/>'), xml, fixed = TRUE),
    grepl(paste0("<CheckValue>", case$code, "</CheckValue>"), xml, fixed = TRUE))
}
stopifnot(count('Comparator="EQ" SoftHard="Soft" def:ItemOID="IT.LB.LBTESTCD"', xml) == 2L)
json <- rawToChar(held[[1L]]$bytes)
stopifnot(grepl('"records": 3', json, fixed = TRUE))
# These fixture strings need no JSON unescaping. Compare all typed row values
# with the immutable CSV, allowing equivalent JSON numeric spellings.
json_lines <- strsplit(json, "\n", fixed = TRUE)[[1L]]
column_lines <- grep('^[[:space:]]*\\{[[:space:]]*"itemOID"', json_lines, value = TRUE)
column_names <- c("DOMAIN", "STUDYID", "USUBJID", "LBSEQ", "LBTESTCD", "LBTEST", "LBORRES", "LBORRESU", "LBSTRESN", "LBSTRESU", "LBDTC")
column_types <- c(rep("string", 3L), "integer", rep("string", 4L), "float", "string", "date")
field <- function(name, lines) {
  pattern <- paste0('.*"', name, '"[[:space:]]*:[[:space:]]*"([^"]*)".*')
  stopifnot(all(grepl(pattern, lines)))
  sub(pattern, "\\1", lines)
}
stopifnot(identical(field("name", column_lines), column_names),
  identical(field("dataType", column_lines), column_types))
row_lines <- grep('^[[:space:]]*\\["LB",', json_lines, value = TRUE)
stopifnot(length(row_lines) == 3L)
row_csv <- sub('^[[:space:]]*\\[', "", sub('\\],?[[:space:]]*$', "", row_lines))
types <- c(rep("character", 3L), "integer", rep("character", 4L), "numeric", rep("character", 2L))
actual <- read.csv(text = paste(row_csv, collapse = "\n"), header = FALSE,
  colClasses = types, strip.white = TRUE, check.names = FALSE)
expected <- read.csv(file.path(fixtures, "sdtm-lb-metadata/expected/lb.csv"),
  colClasses = types, check.names = FALSE)
stopifnot(identical(names(expected), column_names),
  identical(unname(as.list(actual)), unname(as.list(expected))))
inputs <- file.path(root, c("spec.yaml", "submission-spec.yaml", "environment.yaml"))
stopifnot(unlink(inputs) == 0L, !any(file.exists(inputs)))
writeLines("changed after preparation", file.path(root, "lb.csv"))
stopifnot(isTRUE(result$save()), identical(result$save(), FALSE))
for (artifact in held) stopifnot(identical(rawfile(file.path(root, artifact$path)), artifact$bytes))
unlink(root, recursive = TRUE)

for (mode in c("original source", "value codelist", "metadata only", "arbitrary row ID")) {
  metadata_only <- mode %in% c("metadata only", "arbitrary row ID")
  root <- fixture("sdtm-lb-metadata", !metadata_only)
  if (mode == "original source") {
    overlay <- readLines(file.path(root, "submission-spec.yaml"))
    rows <- which(overlay == "rows:")
    stopifnot(length(rows) == 1L)
    writeLines(overlay[seq_len(rows - 1L)], file.path(root, "submission-spec.yaml"))
    stopifnot(identical(rawfile(file.path(root, "spec.yaml")), rawfile(file.path(fixtures, "sdtm-lb-metadata/spec.yaml"))))
  } else if (mode == "value codelist") {
    csv <- readLines(file.path(root, "lb.csv"))
    writeLines(sub("95,mg/dL,95", "95,mmol/L,95", csv, fixed = TRUE), file.path(root, "lb.csv"))
  } else {
    environment <- readLines(file.path(root, "environment.yaml"))
    writeLines(sub("dataset_json: true", "dataset_json: false", environment, fixed = TRUE), file.path(root, "environment.yaml"))
    if (mode == "arbitrary row ID") {
      for (file in c("spec.yaml", "submission-spec.yaml")) {
        path <- file.path(root, file)
        writeLines(sub("id: glucose", "id: 'glucose / primary test: 1'", readLines(path), fixed = TRUE), path)
      }
    }
  }
  result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
  check_issues(result, metadata_only)
  if (mode == "original source") {
    stopifnot(any(result$issues$requirement == "REQ-0919" & result$issues$condition == "origin_source_not_admitted"), refused(result))
  } else if (mode == "value codelist") {
    row <- which(result$issues$condition == "allowed_values_failed")
    stopifnot(length(row) == 1L, grepl('"codelist":"CL_GLUCOSE"', result$issues$context[row], fixed = TRUE),
      grepl('"testcd":"GLUC"', result$issues$context[row], fixed = TRUE), refused(result))
  } else {
    stopifnot(length(artifacts(result)) == 1L, !file.exists(file.path(root, "lb.csv")))
  }
  stopifnot(!file.exists(file.path(root, "define.xml")), !file.exists(file.path(root, "lb.json")), !dir.exists(file.path(root, "input")))
  unlink(root, recursive = TRUE)
}
cat("PASS: independent LB value metadata, original-source rejection and selected-value codelist\n")

for (case in list(
  list(name = "sdtm-ae-coding", id = "AE", columns = c("DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM", "AEDECOD", "AEBODSYS"), context = "Submission", codelist = "MEDDRA", acrf = TRUE),
  list(name = "sdtm-vs-epoch-from-subject-elements", id = "VS", columns = c("DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VSTESTCD", "VSORRES", "VSDTC", "EPOCH"), context = "Other", codelist = "EPOCH", acrf = FALSE))) {
  root <- fixture(case$name)
  result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-30T00:00:00")
  check_issues(result); held <- artifacts(result); stopifnot(length(held) == 1L)
  xml <- rawToChar(held[[1L]]$bytes)
  stopifnot(grepl('CreationDateTime="2026-09-30T00:00:00"', xml, fixed = TRUE),
    grepl('SourceSystem="yamaa" SourceSystemVersion="1.0"', xml, fixed = TRUE),
    grepl(paste0('def:Context="', case$context, '"'), xml, fixed = TRUE),
    grepl(paste0('OID="IG.', case$id, '" Name="', case$id, '"'), xml, fixed = TRUE),
    identical(grepl("<def:AnnotatedCRF>", xml, fixed = TRUE), case$acrf),
    grepl(paste0('OID="CL.', case$codelist, '"'), xml, fixed = TRUE))
  lines <- strsplit(xml, "\n", fixed = TRUE)[[1L]]
  items <- lines[grepl("<ItemDef ", lines, fixed = TRUE)]
  stopifnot(length(items) == length(case$columns))
  for (i in seq_along(items)) stopifnot(grepl(paste0('OID="IT.', case$id, '.', case$columns[[i]], '" Name="', case$columns[[i]], '"'), items[[i]], fixed = TRUE))
  if (case$id == "AE") stopifnot(grepl('Dictionary="MedDRA" Version="26.1"', xml, fixed = TRUE), grepl('OID="MT.AE.AEDECOD"', xml, fixed = TRUE))
  else stopifnot(grepl('Name="VSDTC" DataType="datetime"', xml, fixed = TRUE), grepl('CodedValue="FOLLOW-UP"', xml, fixed = TRUE))
  stopifnot(!file.exists(file.path(root, paste0(tolower(case$id), ".csv"))), isTRUE(result$save()), identical(rawfile(file.path(root, "define.xml")), held[[1L]]$bytes))
  unlink(root, recursive = TRUE)
}

root <- fixture("sdtm-dm-metadata")
result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
check_issues(result, FALSE)
stopifnot("producer_contract_mismatch" %in% result$issues$condition, "REQ-0535" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "dm.json")), !file.exists(file.path(root, "define.xml")))
environment <- file.path(root, "environment.yaml")
writeLines(gsub("dataset_json: true", "dataset_json: false", readLines(environment), fixed = TRUE), environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
writeLines(gsub("name: SDTMIG", "name: ADaMIG", readLines(environment), fixed = TRUE), environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("unknown_standard_family" %in% result$issues$condition, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
result <- yamaa_define(file.path(root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
dir.create(file.path(root, "define.xml"))
stopifnot(refused(result), !file.exists(file.path(root, "dm.json")), identical(result$issues$phase, "engine"), identical(result$issues$condition, "engine_rejected"),
  all(is.na(result$issues$requirement)), grepl("publication_target_admission", result$issues$context, fixed = TRUE), grepl("cause", result$issues$context, fixed = TRUE),
  identical(vapply(artifacts(result), `[[`, character(1), "state"), c("Stopped", "Failed")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
environment <- file.path(root, "environment.yaml")
lines <- readLines(environment); writeLines(lines[!grepl("source_system_version:", lines, fixed = TRUE)], environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
version <- as.character(utils::packageVersion("yamaa"))
stopifnot(grepl(paste0('SourceSystemVersion="', version, '"'), rawToChar(artifacts(result)[[2L]]$bytes), fixed = TRUE))
writeLines(gsub("name: CDISC/NCI", "name: InvalidStandard", readLines(environment), fixed = TRUE), environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("REQ-1026" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
environment <- file.path(root, "environment.yaml")
lines <- readLines(environment)
metadata <- which(lines == "  metadata_version:")
stopifnot(length(metadata) == 1L)
lines <- lines[-(metadata + 0:3)]
lines <- lines[!grepl("^  file_oid:", lines)]
writeLines(c(lines, "adam:", "  standard: {name: ADaMIG, version: '1.3', status: Final}",
  "  specs: [missing-other-section.yaml]", "  define: adam.xml"), environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
xml <- rawToChar(artifacts(result)[[2L]]$bytes)
stopifnot(grepl('FileOID="STUDY01.sdtm.Define-XML_2.1"', xml, fixed = TRUE),
  grepl('OID="MDV.STUDY01.sdtm" Name="STUDY01 sdtm metadata"', xml, fixed = TRUE), !file.exists(file.path(root, "adam.xml")))
lines <- readLines(environment)
acrf <- which(lines == "  - id: acrf"); stopifnot(length(acrf) == 1L)
writeLines(lines[-(acrf + 0:3)], environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("origin_document_missing" %in% result$issues$condition, refused(result), !file.exists(file.path(root, "dm.json")))
writeLines(lines[lines != "    alias: C66734"], environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("REQ-1004" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata")
for (name in c("parents", "children", "artifacts")) dir.create(file.path(root, name))
spec <- readLines(file.path(root, "spec.yaml"))
spec <- gsub("path: dm.csv", "path: ../artifacts/dm.csv", spec, fixed = TRUE)
writeLines(spec, file.path(root, "parents/base.yaml"))
writeLines(c("schema_version: '1.0'", "parents: ../parents/base.yaml"), file.path(root, "children/spec.yaml"))
environment <- file.path(root, "environment.yaml")
writeLines(gsub("define: define.xml", "define: artifacts/define.xml", gsub("  - spec.yaml", "  - children/spec.yaml", readLines(environment), fixed = TRUE), fixed = TRUE), environment)
stopifnot(file.copy(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"), file.path(root, "artifacts/dm.csv")))
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
stopifnot(grepl('xlink:href="dm.json"', rawToChar(artifacts(result)[[2L]]$bytes), fixed = TRUE),
  isTRUE(result$save()), file.exists(file.path(root, "artifacts/dm.json")), !file.exists(file.path(root, "dm.json")))
previous <- rawfile(file.path(root, "artifacts/define.xml"))
writeLines(gsub("values: [F, M, U]", "values: [F, M]", spec, fixed = TRUE), file.path(root, "parents/base.yaml"))
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("codelist_values_conflict" %in% result$issues$condition, "REQ-0955" %in% result$issues$requirement,
  refused(result), identical(rawfile(file.path(root, "artifacts/define.xml")), previous))
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata")
dir.create(file.path(root, "artifacts"))
spec_path <- file.path(root, "spec.yaml"); spec <- readLines(spec_path)
writeLines(gsub("path: dm.csv", "path: artifacts/dm.csv", spec, fixed = TRUE), spec_path)
environment <- file.path(root, "environment.yaml")
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("REQ-1229" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
writeLines(spec, spec_path)
csv <- readLines(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"))
writeLines(gsub("STUDYID,USUBJID", "USUBJID,STUDYID", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("REQ-0535" %in% result$issues$requirement, refused(result))
writeLines(gsub("STUDYID", "WRONG", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
stopifnot("REQ-0535" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
writeLines(gsub(",F,USA", ",Z,USA", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
rows <- result$issues[result$issues$requirement == "REQ-0957", , drop = FALSE]
stopifnot(nrow(rows) == 1L, rows$phase == "verification", grepl('"value":"Z"', rows$context, fixed = TRUE),
  grepl("CATH-UCSD-0001", rows$context, fixed = TRUE), refused(result), !file.exists(file.path(root, "dm.json")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
spec_path <- file.path(root, "spec.yaml")
spec <- paste(readLines(spec_path), collapse = "\n")
spec <- gsub("name: AGE\n    type: int", "name: AGE\n    type: float", spec, fixed = TRUE)
spec <- gsub("core: Exp\n      role: Record Qualifier\n      length: 3", "core: Exp\n      role: Record Qualifier\n      length: 3\n      significant_digits: 3\n      codelist: AGE_VALUES", spec, fixed = TRUE)
writeLines(spec, spec_path)
environment <- file.path(root, "environment.yaml")
source <- paste(readLines(environment), collapse = "\n")
source <- gsub("sdtm:\n", "- codelists:\n  - id: AGE_VALUES\n    name: Ages\n    data_type: float\n    items: [{value: 34}, {value: 67}, {value: 52}]\nsdtm:\n", source, fixed = TRUE)
writeLines(source, environment)
result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00"); check_issues(result)
stopifnot(grepl('"name": "AGE", "label": "Age", "dataType": "float"', rawToChar(artifacts(result)[[1L]]$bytes), fixed = TRUE))
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata", TRUE)
sentinel <- file.path(root, "dm.json")
writeBin(charToRaw("original publication sentinel"), sentinel)
if (file.exists(file.path(root, "DM.JSON"))) {
  environment <- file.path(root, "environment.yaml")
  writeLines(gsub("  - spec.yaml", "  - spec.yaml\n  - second.yaml", readLines(environment), fixed = TRUE), environment)
  spec <- gsub("domain: DM", "domain: DM2", readLines(file.path(root, "spec.yaml")), fixed = TRUE)
  writeLines(gsub("path: dm.csv", "path: DM.csv", spec, fixed = TRUE), file.path(root, "second.yaml"))
  define_sentinel <- file.path(root, "define.xml")
  writeBin(charToRaw("original Define sentinel"), define_sentinel)
  previous_names <- sort(list.files(root, all.files = TRUE, no.. = TRUE))
  result <- yamaa_define(environment, "sdtm", created = "2026-09-11T00:00:00")
  stopifnot(identical(result$issues$phase, "validation"), identical(result$issues$condition, "dataset_json_path_collision"), identical(result$issues$requirement, "REQ-1226"),
    grepl("dm.json", result$issues$context, fixed = TRUE), grepl("DM.json", result$issues$context, fixed = TRUE), refused(result),
    identical(rawfile(sentinel), charToRaw("original publication sentinel")),
    identical(rawfile(define_sentinel), charToRaw("original Define sentinel")),
    identical(sort(list.files(root, all.files = TRUE, no.. = TRUE)), previous_names),
    all(vapply(artifacts(result), `[[`, character(1), "state") == "Stopped"))
}
unlink(root, recursive = TRUE)
Sys.setenv(PATH = original_path)
unlink(native_path, recursive = TRUE)
cat("PASS: shared installed R Define DM goldens, AE/VS structure, issues, held Save, version and zero-write failures\n")
