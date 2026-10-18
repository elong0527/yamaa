# Independent installed R submission witnesses. The runtime has no Python
# executable. project$submission() composes Define-XML (and the Dataset-JSON or
# SAS Transport files where the section sets `datasets: json` or
# `datasets: xpt`) when called; only $save() writes files.
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-define")
fixtures <- system.file("define-fixtures", package = "yamaa", mustWork = TRUE)
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
fixture <- function(name, published = FALSE) {
  root <- tempfile("define-"); dir.create(root)
  stopifnot(file.copy(file.path(fixtures, name, c("spec.yaml", "project.yaml")), root))
  if (name == "sdtm-lb-metadata") stopifnot(file.copy(file.path(fixtures, name, "submission-spec.yaml"), root))
  if (published) {
    dataset <- if (name == "sdtm-lb-metadata") "lb" else "dm"
    stopifnot(file.copy(file.path(fixtures, name, "expected", paste0(dataset, ".csv")), file.path(root, paste0(dataset, ".csv"))))
  }
  root
}
edit_lines <- function(path, edit) writeLines(edit(readLines(path)), path)
# Compose Define-XML alone: the section no longer asks for Dataset-JSON.
metadata_only <- function(root) edit_lines(file.path(root, "project.yaml"),
  function(lines) lines[lines != "  datasets: json"])
submission <- function(root, created = host_truth[["created"]], section = "sdtm") {
  result <- yamaa_project(file.path(root, "project.yaml"))$submission(section, created = created)
  check_issues(result, FALSE)
  result
}
check_issues <- function(result, empty = TRUE) {
  frame <- result$issues
  stopifnot(is.data.frame(frame), identical(names(frame), fixture_strings(host_truth[["issue_columns"]])),
    all(vapply(frame[c("phase", "condition", "requirement", "context")], is.character, logical(1))),
    is.list(frame$spec_paths), all(vapply(frame$source_span, is.null, logical(1))),
    all(vapply(frame$operand_route, is.null, logical(1))))
  if (empty) stopifnot(nrow(frame) == 0L)
}
refused <- function(result) tryCatch({ result$save(); FALSE }, yamaa_error = function(e) {
  stopifnot(identical(e$issues, result$issues)); TRUE
})
# Save, require exactly `names` to appear, and return their bytes.
saved <- function(result, root, names) {
  before <- list.files(root, all.files = TRUE, no.. = TRUE)
  stopifnot(is.null(result$save()),
    identical(sort(setdiff(list.files(root, all.files = TRUE, no.. = TRUE), before)), sort(names)))
  setNames(lapply(file.path(root, names), rawfile), names)
}
original_path <- Sys.getenv("PATH")
native_path <- tempfile("define-runtime-"); dir.create(native_path)
rm_path <- Sys.which("rm")
if (nzchar(rm_path)) invisible(file.symlink(rm_path, file.path(native_path, "rm")))
Sys.setenv(PATH = native_path)
stopifnot(!nzchar(Sys.which("python")), !nzchar(Sys.which("python3")))

root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root)
check_issues(result)
stopifnot(!file.exists(file.path(root, "dm.json")), !file.exists(file.path(root, "define.xml")))
# The composition is held: sources may change or go before save.
unlink(file.path(root, c("spec.yaml", "project.yaml")))
writeLines("changed after preparation", file.path(root, "dm.csv"))
held <- saved(result, root, c("dm.json", "define.xml"))
for (name in names(held)) stopifnot(identical(held[[name]], rawfile(file.path(fixtures, "sdtm-dm-metadata/expected", name))))
# Every save rewrites the held bytes: deleted files come back unchanged.
unlink(file.path(root, names(held)))
stopifnot(identical(saved(result, root, c("dm.json", "define.xml")), held))
unlink(root, recursive = TRUE)

# Without a section every declared define section is composed; a section the
# project does not declare is a submission issue.
root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root, section = NULL); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
for (name in names(held)) stopifnot(identical(held[[name]], rawfile(file.path(fixtures, "sdtm-dm-metadata/expected", name))))
absent <- submission(root, section = "adam")
stopifnot(identical(absent$issues$condition, "missing_required_field"), identical(absent$issues$requirement, "REQ-1337"),
  identical(absent$issues$spec_paths, list("adam")), refused(absent))
unlink(root, recursive = TRUE)

# The original LB spec and expected CSV remain immutable. The separate metadata
# completion explicitly supplies SDTM-valid sources and complete column facts.
root <- fixture("sdtm-lb-metadata", TRUE)
result <- submission(root); check_issues(result)
inputs <- file.path(root, fixture_strings(host_truth[["retained_source_files"]]))
stopifnot(unlink(inputs) == 0L, !any(file.exists(inputs)))
writeLines("changed after preparation", file.path(root, "lb.csv"))
held <- saved(result, root, c("lb.json", "define.xml"))
xml <- rawToChar(held[["define.xml"]])
count <- function(pattern, text) sum(gregexpr(pattern, text, fixed = TRUE)[[1L]] > 0L)
stopifnot(count("<def:ValueListDef ", xml) == 1L, count("<def:ValueListRef ", xml) == 1L,
  count("<def:WhereClauseDef ", xml) == 2L, count("<ItemDef ", xml) == 13L,
  grepl('CreationDateTime="2026-09-11T00:00:00"', xml, fixed = TRUE),
  grepl('SourceSystemVersion="1.0"', xml, fixed = TRUE),
  grepl('<def:ValueListRef ValueListOID="VL.LB.LBORRESU"/>', xml, fixed = TRUE))
for (case in list(list(i = 1L, code = "GLUC", source = "Vendor", list = "CL_GLUCOSE"),
  list(i = 2L, code = "CREAT", source = "Sponsor", list = "CL_CREATININE"))) {
  oid <- paste0("LB.LBORRESU.", case$code)
  item_start <- regexpr(paste0('<ItemDef OID="IT.', oid, '"'), xml, fixed = TRUE)[[1L]]
  stopifnot(item_start > 0L)
  item <- substring(xml, item_start)
  item <- substring(item, 1L, regexpr("</ItemDef>", item, fixed = TRUE)[[1L]])
  # Every unit of both test codes is `mg/dL`, five bytes (SUB-24).
  stopifnot(grepl('Name="LBORRESU" DataType="text" Length="5"', item, fixed = TRUE),
    grepl(paste0('CodeListOID="CL.', case$list, '"'), item, fixed = TRUE),
    grepl(paste0('<def:Origin Type="Assigned" Source="', case$source, '"/>'), item, fixed = TRUE),
    grepl(paste0('<def:WhereClauseRef WhereClauseOID="WC.', oid, '"/>'), xml, fixed = TRUE),
    grepl(paste0("<CheckValue>", case$code, "</CheckValue>"), xml, fixed = TRUE))
}
stopifnot(count('Comparator="EQ" SoftHard="Soft" def:ItemOID="IT.LB.LBTESTCD"', xml) == 2L)
json <- rawToChar(held[["lb.json"]])
stopifnot(grepl('"records": 3', json, fixed = TRUE))
# These fixture strings need no JSON unescaping. Compare all typed row values
# with the immutable CSV, allowing equivalent JSON numeric spellings.
json_lines <- strsplit(json, "\n", fixed = TRUE)[[1L]]
column_lines <- grep('^[[:space:]]*\\{[[:space:]]*"itemOID"', json_lines, value = TRUE)
column_names <- fixture_strings(host_truth[["lb_columns"]])
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
unlink(root, recursive = TRUE)

for (mode in c("original source", "value codelist", "metadata only", "arbitrary row ID")) {
  metadata <- mode %in% c("metadata only", "arbitrary row ID")
  root <- fixture("sdtm-lb-metadata", !metadata)
  if (mode == "original source") {
    overlay <- readLines(file.path(root, "submission-spec.yaml"))
    start <- which(overlay == "      value_level:")
    stopifnot(length(start) == 1L)
    # Drop the completion's value_level block only, so the creatinine entry
    # inherits the original Investigator source.
    after <- seq.int(start + 1L, length(overlay))
    block <- after[cumprod(grepl("^        ", overlay[after])) == 1L]
    writeLines(overlay[-c(start, block)], file.path(root, "submission-spec.yaml"))
    stopifnot(identical(rawfile(file.path(root, "spec.yaml")), rawfile(file.path(fixtures, "sdtm-lb-metadata/spec.yaml"))))
  } else if (mode == "value codelist") {
    csv <- readLines(file.path(root, "lb.csv"))
    writeLines(sub("95,mg/dL,95", "95,mmol/L,95", csv, fixed = TRUE), file.path(root, "lb.csv"))
  } else {
    metadata_only(root)
    if (mode == "arbitrary row ID") {
      for (file in c("spec.yaml", "submission-spec.yaml")) {
        path <- file.path(root, file)
        writeLines(sub("id: glucose", "id: 'glucose / primary test: 1'", readLines(path), fixed = TRUE), path)
      }
    }
  }
  result <- submission(root)
  if (mode == "original source") {
    stopifnot(any(result$issues$requirement == "REQ-0919" & result$issues$condition == "origin_source_not_admitted"), refused(result))
  } else if (mode == "value codelist") {
    row <- which(result$issues$condition == "allowed_values_failed")
    stopifnot(length(row) == 1L, grepl('"codelist":"CL_GLUCOSE"', result$issues$context[row], fixed = TRUE),
      grepl('"entry":"GLUC"', result$issues$context[row], fixed = TRUE), refused(result))
  } else {
    check_issues(result)
    saved(result, root, "define.xml")
    stopifnot(!file.exists(file.path(root, "lb.csv")))
  }
  stopifnot(!file.exists(file.path(root, "lb.json")), !dir.exists(file.path(root, "input")),
    metadata || !file.exists(file.path(root, "define.xml")))
  unlink(root, recursive = TRUE)
}
cat("PASS: independent LB value metadata, original-source rejection and selected-value codelist\n")

for (case in list(
  list(name = "sdtm-ae-coding", id = "AE", columns = fixture_strings(host_truth[["ae_columns"]]), context = "Submission", codelist = "MEDDRA", acrf = TRUE),
  list(name = "sdtm-vs-epoch-from-subject-elements", id = "VS", columns = fixture_strings(host_truth[["vs_columns"]]), context = "Other", codelist = "EPOCH", acrf = FALSE))) {
  root <- fixture(case$name)
  result <- submission(root, created = "2026-09-30T00:00:00"); check_issues(result)
  stopifnot(!file.exists(file.path(root, paste0(tolower(case$id), ".csv"))))
  xml <- rawToChar(saved(result, root, "define.xml")[["define.xml"]])
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
  else stopifnot(grepl('Name="VSDTC" DataType="partialDatetime"', xml, fixed = TRUE), grepl('CodedValue="FOLLOW-UP"', xml, fixed = TRUE))
  unlink(root, recursive = TRUE)
}

root <- fixture("sdtm-dm-metadata")
result <- submission(root)
stopifnot("producer_contract_mismatch" %in% result$issues$condition, "REQ-0535" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "dm.json")), !file.exists(file.path(root, "define.xml")))
metadata_only(root)
result <- submission(root); check_issues(result)
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) gsub("name: SDTMIG", "name: ADaMIG", lines, fixed = TRUE))
result <- submission(root)
stopifnot("unknown_standard_family" %in% result$issues$condition, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root); check_issues(result)
dir.create(file.path(root, "define.xml"))
error <- tryCatch(result$save(), yamaa_error = identity)
stopifnot(inherits(error, "yamaa_error"), identical(error$issues, result$issues),
  !file.exists(file.path(root, "dm.json")), dir.exists(file.path(root, "define.xml")),
  identical(result$issues$phase, "engine"), identical(result$issues$condition, "engine_rejected"),
  all(is.na(result$issues$requirement)), grepl("publication_target_admission", result$issues$context, fixed = TRUE),
  grepl("cause", result$issues$context, fixed = TRUE))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) lines[!grepl("source_system_version:", lines, fixed = TRUE)])
result <- submission(root); check_issues(result)
version <- yamaa:::.core_version()
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot(grepl(paste0('SourceSystemVersion="', version, '"'), rawToChar(held[["define.xml"]]), fixed = TRUE),
  grepl(paste0('"version": "', version, '"'), rawToChar(held[["dm.json"]]), fixed = TRUE))
unlink(file.path(root, names(held)))
edit_lines(project, function(lines) gsub("name: CDISC/NCI", "name: InvalidStandard", lines, fixed = TRUE))
result <- submission(root)
stopifnot("REQ-1026" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
project <- file.path(root, "project.yaml")
lines <- readLines(project)
metadata <- which(lines == "  metadata_version:")
stopifnot(length(metadata) == 1L)
lines <- lines[-(metadata + 0:3)]
lines <- lines[!grepl("^  file_oid:", lines)]
writeLines(c(lines, "adam:", "  standard: {name: ADaMIG, version: '1.3', status: Final}",
  "  specs: [missing-other-section.yaml]", "  define: adam.xml"), project)
result <- submission(root); check_issues(result)
xml <- rawToChar(saved(result, root, c("dm.json", "define.xml"))[["define.xml"]])
stopifnot(grepl('FileOID="STUDY01.sdtm.Define-XML_2.1"', xml, fixed = TRUE),
  grepl('OID="MDV.STUDY01.sdtm" Name="STUDY01 sdtm metadata"', xml, fixed = TRUE), !file.exists(file.path(root, "adam.xml")))
unlink(file.path(root, c("dm.json", "define.xml")))
lines <- readLines(project)
acrf <- which(lines == "  - id: acrf"); stopifnot(length(acrf) == 1L)
writeLines(lines[-(acrf + 0:3)], project)
result <- submission(root)
stopifnot("origin_document_missing" %in% result$issues$condition, refused(result), !file.exists(file.path(root, "dm.json")))
writeLines(lines[lines != "    alias: C66734"], project)
result <- submission(root)
stopifnot("REQ-1004" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata")
for (name in c("parents", "children", "artifacts")) dir.create(file.path(root, name))
spec <- readLines(file.path(root, "spec.yaml"))
spec <- gsub("path: dm.csv", "path: ../artifacts/dm.csv", spec, fixed = TRUE)
writeLines(spec, file.path(root, "parents/base.yaml"))
writeLines(c("schema_version: '1.0'", "parent: ../parents/base.yaml"), file.path(root, "children/spec.yaml"))
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) gsub("define: define.xml", "define: artifacts/define.xml",
  gsub("  - spec.yaml", "  - children/spec.yaml", lines, fixed = TRUE), fixed = TRUE))
stopifnot(file.copy(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"), file.path(root, "artifacts/dm.csv")))
result <- submission(root); check_issues(result)
held <- saved(result, file.path(root, "artifacts"), c("dm.json", "define.xml"))
stopifnot(grepl('xlink:href="dm.json"', rawToChar(held[["define.xml"]]), fixed = TRUE),
  grepl('"metaDataRef": "define.xml"', rawToChar(held[["dm.json"]]), fixed = TRUE),
  !file.exists(file.path(root, "dm.json")))
# The SEX codelist lists its items, so an allowed_values beside it states the same value set twice.
sex <- which(spec == "    derivation: DM_RAW.SEX"); stopifnot(length(sex) == 1L)
writeLines(append(spec, c("    verifications:", "      - allowed_values:", "          values: [F, M, U]"), after = sex),
  file.path(root, "parents/base.yaml"))
result <- submission(root)
stopifnot("redundant_declaration" %in% result$issues$condition, "REQ-0955" %in% result$issues$requirement,
  refused(result), identical(rawfile(file.path(root, "artifacts/define.xml")), held[["define.xml"]]))
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata")
dir.create(file.path(root, "artifacts"))
spec_path <- file.path(root, "spec.yaml"); spec <- readLines(spec_path)
writeLines(gsub("path: dm.csv", "path: artifacts/dm.csv", spec, fixed = TRUE), spec_path)
stopifnot(file.copy(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"), file.path(root, "artifacts/dm.csv")))
# The transport file sits beside the document wherever the artifact is.
result <- submission(root); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot(grepl('xlink:href="dm.json"', rawToChar(held[["define.xml"]]), fixed = TRUE),
  !file.exists(file.path(root, "artifacts/dm.json")))
unlink(file.path(root, c("dm.json", "define.xml")))
writeLines(spec, spec_path)
csv <- readLines(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"))
writeLines(gsub("STUDYID,USUBJID", "USUBJID,STUDYID", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
stopifnot("REQ-0535" %in% result$issues$requirement, refused(result))
writeLines(gsub("STUDYID", "WRONG", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
stopifnot("REQ-0535" %in% result$issues$requirement, refused(result), !file.exists(file.path(root, "define.xml")))
writeLines(gsub(",F,USA", ",Z,USA", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
rows <- result$issues[result$issues$requirement %in% "REQ-0957", , drop = FALSE]
stopifnot(nrow(rows) == 1L, rows$phase == "verification", grepl('"value":"Z"', rows$context, fixed = TRUE),
  grepl("CATH-UCSD-0001", rows$context, fixed = TRUE), refused(result), !file.exists(file.path(root, "dm.json")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
spec_path <- file.path(root, "spec.yaml")
spec <- paste(readLines(spec_path), collapse = "\n")
spec <- gsub("name: AGE\n    type: int", "name: AGE\n    type: float", spec, fixed = TRUE)
spec <- gsub(host_truth[["numeric_original_metadata"]], host_truth[["numeric_completion_metadata"]], spec, fixed = TRUE)
writeLines(spec, spec_path)
project <- file.path(root, "project.yaml")
source <- paste(readLines(project), collapse = "\n")
source <- gsub("sdtm:\n", host_truth[["numeric_codelist_yaml"]], source, fixed = TRUE)
writeLines(source, project)
result <- submission(root); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot(grepl('"name": "AGE", "label": "Age", "dataType": "float"', rawToChar(held[["dm.json"]]), fixed = TRUE))
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata", TRUE)
sentinel <- file.path(root, "dm.json")
writeBin(charToRaw("original publication sentinel"), sentinel)
# Under Other a lower-case dataset name is admitted, and its transport file is
# named for it in lower case beside the document.
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) gsub("context: Submission", "context: Other",
  gsub("  - spec.yaml", "  - spec.yaml\n  - second.yaml", lines, fixed = TRUE), fixed = TRUE))
spec <- gsub("domain: DM", "domain: dm", readLines(file.path(root, "spec.yaml")), fixed = TRUE)
writeLines(gsub("path: dm.csv", "path: second.csv", spec, fixed = TRUE), file.path(root, "second.yaml"))
define_sentinel <- file.path(root, "define.xml")
writeBin(charToRaw("original Define sentinel"), define_sentinel)
previous_names <- sort(list.files(root, all.files = TRUE, no.. = TRUE))
result <- submission(root)
stopifnot(identical(result$issues$phase, "validation"), identical(result$issues$condition, "transport_path_collision"), identical(result$issues$requirement, "REQ-1226"),
  identical(result$issues$spec_paths[[1L]], "sdtm.specs[1]"), grepl("dm.json", result$issues$context, fixed = TRUE), refused(result),
  identical(rawfile(sentinel), charToRaw("original publication sentinel")),
  identical(rawfile(define_sentinel), charToRaw("original Define sentinel")),
  identical(sort(list.files(root, all.files = TRUE, no.. = TRUE)), previous_names))
unlink(root, recursive = TRUE)
for (case in host_truth[["xml_text_cases"]]) {
  root <- fixture("sdtm-dm-metadata")
  metadata_only(root)
  for (filename in c("spec.yaml", "project.yaml")) {
    path <- file.path(root, filename)
    text <- paste(readLines(path), collapse = "\n")
    mutations <- if (filename == "spec.yaml") case[["specification_mutations"]] else case[["project_mutations"]]
    for (mutation in mutations) {
      stopifnot(grepl(mutation[[1L]], text, fixed = TRUE))
      text <- sub(mutation[[1L]], mutation[[2L]], text, fixed = TRUE)
    }
    writeLines(text, path)
  }
  if (case[["inherit_specification"]]) {
    stopifnot(file.rename(file.path(root, "spec.yaml"), file.path(root, "base.yaml")))
    writeLines(c("schema_version: '1.0'", "parent: base.yaml"), file.path(root, "spec.yaml"))
  }
  result <- submission(root)
  check_issues(result, empty = !length(case[["issues"]]))
  stopifnot(nrow(result$issues) == length(case[["issues"]]),
    !file.exists(file.path(root, "define.xml")), !file.exists(file.path(root, "dm.json")))
  for (i in seq_along(case[["issues"]])) {
    expected <- case[["issues"]][[i]]
    stopifnot(identical(result$issues$phase[[i]], expected[["phase"]]),
      identical(result$issues$condition[[i]], expected[["condition"]]),
      identical(result$issues$requirement[[i]], expected[["requirement"]]),
      identical(result$issues$spec_paths[[i]], fixture_strings(expected[["spec_paths"]])),
      context_equal(jsonlite::fromJSON(result$issues$context[[i]], simplifyVector = FALSE), expected[["context"]]))
  }
  if (length(case[["issues"]])) {
    stopifnot(refused(result), !file.exists(file.path(root, "define.xml")))
  } else {
    xml <- rawToChar(saved(result, root, "define.xml")[["define.xml"]])
    for (fragment in case[["xml_fragments"]]) stopifnot(grepl(fragment, xml, fixed = TRUE))
  }
  stopifnot(!dir.exists(file.path(root, "input")), !file.exists(file.path(root, "dm.csv")))
  unlink(root, recursive = TRUE)
}
# Every benchmark declaring a Define call: positive cases hold and save the
# expected bytes; negative cases fail with the expected first issue.
manifest <- jsonlite::fromJSON(file.path(fixtures, "benchmarks", "manifest.json"), simplifyVector = FALSE)
case_names <- vapply(manifest$cases, `[[`, character(1), "name")
stopifnot(all(c("sdtm-dm-xpt", "adam-adsl-xpt", "sdtm-dm-metadata") %in% case_names))
for (case in manifest$cases) {
  source <- file.path(fixtures, "benchmarks", case$name)
  root <- tempfile(paste0(case$name, "-")); dir.create(root)
  files <- list.files(source)
  files <- files[!dir.exists(file.path(source, files))]
  stopifnot(all(file.copy(file.path(source, files), root)))
  for (name in unlist(case$published)) stopifnot(file.copy(file.path(source, "expected", name), file.path(root, name)))
  result <- yamaa_project(file.path(root, case$project))$submission(case$standard, created = case$created)
  if (!is.null(case$error)) {
    want <- case$error
    stopifnot(nrow(result$issues) >= 1L, identical(result$issues$phase[[1L]], want$phase),
      identical(result$issues$condition[[1L]], want$condition),
      identical(result$issues$requirement[[1L]], want$requirement),
      identical(result$issues$spec_paths[[1L]], fixture_strings(want$spec_paths)))
    context <- jsonlite::fromJSON(result$issues$context[[1L]], simplifyVector = FALSE)
    for (key in names(want$context)) stopifnot(identical(context[[key]], want$context[[key]]))
    stopifnot(refused(result), !file.exists(file.path(root, "define.xml")))
  } else {
    check_issues(result)
    held <- saved(result, root, unlist(case$expected))
    for (name in names(held)) stopifnot(identical(held[[name]], rawfile(file.path(source, "expected", name))))
  }
  unlink(root, recursive = TRUE)
}
Sys.setenv(PATH = original_path)
unlink(native_path, recursive = TRUE)
cat("PASS: shared installed R submission DM goldens, AE/VS structure, issues, held save, version and zero-write failures\n")
