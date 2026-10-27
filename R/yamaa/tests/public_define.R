# Independent installed R submission witnesses. The runtime has no Python
# executable. project$submission() composes Define-XML (and the Dataset-JSON or
# SAS Transport files where the section sets `transport: dataset_json` or
# `transport: xpt`) when called; only $save() writes files.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped public_define.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"public-define")
fixtures <- normalizePath("define-fixtures", winslash = "/", mustWork = TRUE)
fixture <- function(name, published = FALSE) {
  root <- tempfile("define-"); dir.create(root)
  stopifnot_case(name, "the fixture's files are copied" =
    all(file.copy(file.path(fixtures, name, c("spec.yaml", "project.yaml")), root)))
  if (name == "sdtm-lb-metadata") stopifnot_case(name, "the completion is copied" =
    file.copy(file.path(fixtures, name, "submission-spec.yaml"), root))
  if (published) {
    dataset <- if (name == "sdtm-lb-metadata") "lb" else "dm"
    stopifnot_case(name, "the published dataset is copied" =
      file.copy(file.path(fixtures, name, "expected", paste0(dataset, ".csv")), file.path(root, paste0(dataset, ".csv"))))
  }
  root
}
edit_lines <- function(path, edit) writeLines(edit(readLines(path)), path)
# Compose Define-XML alone: the section no longer asks for Dataset-JSON.
metadata_only <- function(root) edit_lines(file.path(root, "project.yaml"),
  function(lines) lines[lines != "  transport: dataset_json"])
submission <- function(root, created = host_truth[["created"]], section = "sdtm") {
  result <- yamaa_project(file.path(root, "project.yaml"))$submission(section, created = created)
  check_issues(result, FALSE)
  result
}
check_issues <- function(result, empty = TRUE) {
  frame <- result$issues
  stopifnot("the issues are a data frame" = is.data.frame(frame))
  expect_identical(names(frame), fixture_strings(host_truth[["issue_columns"]]), "the issue columns")
  stopifnot("the text columns are character" =
      all(vapply(frame[c("phase", "condition", "rule", "context")], is.character, logical(1))),
    "spec paths are a list column" = is.list(frame$spec_paths),
    "a submission issue has no span" = all(vapply(frame$source_span, is.null, logical(1))),
    "a submission issue has no route" = all(vapply(frame$operand_route, is.null, logical(1))))
  if (empty && nrow(frame) != 0L) fail_expectation("the submission has no issues", as.list(frame[c("condition", "context")]), NULL)
}
refused <- function(result) tryCatch({ result$save(); FALSE }, yamaa_error = function(e) {
  expect_identical(e$issues, result$issues, "a refused save carries the submission's issues"); TRUE
})
# Save, require exactly `names` to appear, and return their bytes.
saved <- function(result, root, names) {
  before <- list.files(root, all.files = TRUE, no.. = TRUE)
  stopifnot("the submission saves" = returns_paths(result$save()))
  expect_identical(sort(setdiff(list.files(root, all.files = TRUE, no.. = TRUE), before)), sort(names),
    "the files a save writes")
  setNames(lapply(file.path(root, names), rawfile), names)
}
restore_path <- without_python()

root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root)
check_issues(result)
stopifnot("composing writes no Dataset-JSON" = !file.exists(file.path(root, "dm.json")),
  "composing writes no Define-XML" = !file.exists(file.path(root, "define.xml")))
# The composition is held: sources may change or go before save.
unlink(file.path(root, c("spec.yaml", "project.yaml")))
writeLines("changed after preparation", file.path(root, "dm.csv"))
held <- saved(result, root, c("dm.json", "define.xml"))
for (name in names(held)) expect_identical(held[[name]], rawfile(file.path(fixtures, "sdtm-dm-metadata/expected", name)),
  paste("the held submission saves the authored", name))
# Every save rewrites the held bytes: deleted files come back unchanged.
unlink(file.path(root, names(held)))
expect_identical(saved(result, root, c("dm.json", "define.xml")), held, "a second save rewrites the same bytes")
unlink(root, recursive = TRUE)

# Without a section every declared define section is composed; a section the
# project does not declare refuses the call, naming those it declares
# (PROJECT-14).
root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root, section = NULL); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
for (name in names(held)) expect_identical(held[[name]], rawfile(file.path(fixtures, "sdtm-dm-metadata/expected", name)),
  paste("every declared section saves the authored", name))
absent <- tryCatch({ submission(root, section = "adam"); NULL }, yamaa_error = identity)
stopifnot("an undeclared section signals yamaa_error" = inherits(absent, "yamaa_error"))
expect_identical(as.list(absent$issues[c("condition", "rule", "spec_paths", "context")]),
  list(condition = "unknown_section", rule = "PROJECT-14", spec_paths = list(character()),
    context = '{"section":"adam","sections":["sdtm"]}'),
  "an undeclared section's one issue")
expect_identical(absent$paths, character(), "an undeclared section writes nothing")
unlink(root, recursive = TRUE)

# The original LB spec and expected CSV remain immutable. The separate metadata
# completion explicitly supplies SDTM-valid sources and complete column facts.
root <- fixture("sdtm-lb-metadata", TRUE)
result <- submission(root); check_issues(result)
inputs <- file.path(root, fixture_strings(host_truth[["retained_source_files"]]))
stopifnot("the retained sources are removed" = unlink(inputs) == 0L && !any(file.exists(inputs)))
writeLines("changed after preparation", file.path(root, "lb.csv"))
held <- saved(result, root, c("lb.json", "define.xml"))
xml <- rawToChar(held[["define.xml"]])
count <- function(pattern, text) sum(gregexpr(pattern, text, fixed = TRUE)[[1L]] > 0L)
expect_identical(vapply(c("<def:ValueListDef ", "<def:ValueListRef ", "<def:WhereClauseDef ", "<ItemDef "),
  count, integer(1), xml), c("<def:ValueListDef " = 1L, "<def:ValueListRef " = 1L,
  "<def:WhereClauseDef " = 2L, "<ItemDef " = 13L), "the LB Define-XML's value-level elements")
stopifnot("the creation time given" = grepl('CreationDateTime="2026-09-11T00:00:00"', xml, fixed = TRUE),
  "the declared source system version" = grepl('SourceSystemVersion="1.0"', xml, fixed = TRUE),
  "LBORRESU references its value list" = grepl('<def:ValueListRef ValueListOID="VL.LB.LBORRESU"/>', xml, fixed = TRUE))
for (case in list(list(i = 1L, code = "GLUC", source = "Vendor", list = "CL_GLUCOSE"),
  list(i = 2L, code = "CREAT", source = "Sponsor", list = "CL_CREATININE"))) {
  oid <- paste0("LB.LBORRESU.", case$code)
  item_start <- regexpr(paste0('<ItemDef OID="IT.', oid, '"'), xml, fixed = TRUE)[[1L]]
  stopifnot_case(oid, "the value-level ItemDef exists" = item_start > 0L)
  item <- substring(xml, item_start)
  item <- substring(item, 1L, regexpr("</ItemDef>", item, fixed = TRUE)[[1L]])
  # Every unit of both test codes is `mg/dL`, five bytes (SUB-24).
  stopifnot_case(oid, "the unit is text of five bytes" =
      grepl('Name="LBORRESU" DataType="text" Length="5"', item, fixed = TRUE),
    "the value's codelist" = grepl(paste0('CodeListOID="CL.', case$list, '"'), item, fixed = TRUE),
    "the value's origin" = grepl(paste0('<def:Origin Type="Assigned" Source="', case$source, '"/>'), item, fixed = TRUE),
    "the value's where clause" =
      grepl(paste0('<def:WhereClauseRef WhereClauseOID="WC.', oid, '"/>'), xml, fixed = TRUE),
    "the where clause checks the test code" = grepl(paste0("<CheckValue>", case$code, "</CheckValue>"), xml, fixed = TRUE))
}
expect_identical(count('Comparator="EQ" SoftHard="Soft" def:ItemOID="IT.LB.LBTESTCD"', xml), 2L,
  "two where clauses compare LBTESTCD")
json <- rawToChar(held[["lb.json"]])
stopifnot("the LB Dataset-JSON holds three records" = grepl('"records": 3', json, fixed = TRUE))
# These fixture strings need no JSON unescaping. Compare all typed row values
# with the immutable CSV, allowing equivalent JSON numeric spellings.
json_lines <- strsplit(json, "\n", fixed = TRUE)[[1L]]
column_lines <- grep('^[[:space:]]*\\{[[:space:]]*"itemOID"', json_lines, value = TRUE)
column_names <- fixture_strings(host_truth[["lb_columns"]])
column_types <- c(rep("string", 3L), "integer", rep("string", 4L), "float", "string", "date")
field <- function(name, lines) {
  pattern <- paste0('.*"', name, '"[[:space:]]*:[[:space:]]*"([^"]*)".*')
  stopifnot_case(name, "every column line has the field" = all(grepl(pattern, lines)))
  sub(pattern, "\\1", lines)
}
expect_identical(field("name", column_lines), column_names, "the LB Dataset-JSON's column names")
expect_identical(field("dataType", column_lines), column_types, "the LB Dataset-JSON's column types")
row_lines <- grep('^[[:space:]]*\\["LB",', json_lines, value = TRUE)
expect_identical(length(row_lines), 3L, "the LB Dataset-JSON's row lines")
row_csv <- sub('^[[:space:]]*\\[', "", sub('\\],?[[:space:]]*$', "", row_lines))
types <- c(rep("character", 3L), "integer", rep("character", 4L), "numeric", rep("character", 2L))
actual <- read.csv(text = paste(row_csv, collapse = "\n"), header = FALSE,
  colClasses = types, strip.white = TRUE, check.names = FALSE)
expected <- read.csv(file.path(fixtures, "sdtm-lb-metadata/expected/lb.csv"),
  colClasses = types, check.names = FALSE)
expect_identical(names(expected), column_names, "the authored LB CSV's columns")
expect_identical(unname(as.list(actual)), unname(as.list(expected)), "the LB Dataset-JSON's typed rows")
unlink(root, recursive = TRUE)

for (mode in c("original source", "value codelist", "metadata only", "arbitrary row ID")) {
  metadata <- mode %in% c("metadata only", "arbitrary row ID")
  root <- fixture("sdtm-lb-metadata", !metadata)
  if (mode == "original source") {
    overlay <- readLines(file.path(root, "submission-spec.yaml"))
    start <- which(overlay == "      value_level:")
    stopifnot_case(mode, "the completion has one value_level block" = length(start) == 1L)
    # Drop the completion's value_level block only, so the creatinine entry
    # inherits the original Investigator source.
    after <- seq.int(start + 1L, length(overlay))
    block <- after[cumprod(grepl("^        ", overlay[after])) == 1L]
    writeLines(overlay[-c(start, block)], file.path(root, "submission-spec.yaml"))
    expect_identical(rawfile(file.path(root, "spec.yaml")), rawfile(file.path(fixtures, "sdtm-lb-metadata/spec.yaml")),
      paste(mode, "keeps the original LB spec"))
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
    stopifnot_case(mode, "the original source is not admitted" =
        any(result$issues$rule == "SUB-15" & result$issues$condition == "origin_source_not_admitted"),
      "its save is refused" = refused(result))
  } else if (mode == "value codelist") {
    row <- which(result$issues$condition == "allowed_values_failed")
    stopifnot_case(mode, "one value fails its codelist" = length(row) == 1L,
      "the failure names CL_GLUCOSE" = grepl('"codelist":"CL_GLUCOSE"', result$issues$context[row], fixed = TRUE),
      "the failure names GLUC" = grepl('"entry":"GLUC"', result$issues$context[row], fixed = TRUE),
      "its save is refused" = refused(result))
  } else {
    check_issues(result)
    saved(result, root, "define.xml")
    stopifnot_case(mode, "metadata alone reads no dataset" = !file.exists(file.path(root, "lb.csv")))
  }
  stopifnot_case(mode, "no Dataset-JSON is written" = !file.exists(file.path(root, "lb.json")),
    "no input directory is created" = !dir.exists(file.path(root, "input")),
    "a refused submission writes no Define-XML" = metadata || !file.exists(file.path(root, "define.xml")))
  unlink(root, recursive = TRUE)
}
cat("PASS: independent LB value metadata, original-source rejection and selected-value codelist\n")

for (case in list(
  list(name = "sdtm-ae-coding", id = "AE", columns = fixture_strings(host_truth[["ae_columns"]]), context = "Other", codelist = "MEDDRA"),
  list(name = "sdtm-vs-epoch-from-subject-elements", id = "VS", columns = fixture_strings(host_truth[["vs_columns"]]), context = "Other", codelist = "EPOCH"))) {
  root <- fixture(case$name)
  result <- submission(root, created = "2026-09-30T00:00:00"); check_issues(result)
  stopifnot_case(case$name, "metadata alone reads no dataset" = !file.exists(file.path(root, paste0(tolower(case$id), ".csv"))))
  xml <- rawToChar(saved(result, root, "define.xml")[["define.xml"]])
  stopifnot_case(case$name, "the creation time given" = grepl('CreationDateTime="2026-09-30T00:00:00"', xml, fixed = TRUE),
    "the source system and version" = grepl('SourceSystem="yamaa" SourceSystemVersion="1.0"', xml, fixed = TRUE),
    "the declared context" = grepl(paste0('def:Context="', case$context, '"'), xml, fixed = TRUE),
    "the dataset's ItemGroupDef" = grepl(paste0('OID="IG.', case$id, '" Name="', case$id, '"'), xml, fixed = TRUE),
    # Each project declares the annotated case report form its collected
    # values reference.
    "the annotated CRF" = grepl("<def:AnnotatedCRF>", xml, fixed = TRUE),
    "the declared codelist" = grepl(paste0('OID="CL.', case$codelist, '"'), xml, fixed = TRUE))
  lines <- strsplit(xml, "\n", fixed = TRUE)[[1L]]
  items <- lines[grepl("<ItemDef ", lines, fixed = TRUE)]
  expect_identical(length(items), length(case$columns), paste(case$name, "has one ItemDef per column"))
  for (i in seq_along(items)) stopifnot_case(paste(case$name, "column", case$columns[[i]]), "the ItemDef in column order" =
    grepl(paste0('OID="IT.', case$id, '.', case$columns[[i]], '" Name="', case$columns[[i]], '"'), items[[i]], fixed = TRUE))
  if (case$id == "AE") stopifnot_case(case$name, "the MedDRA dictionary" = grepl('Dictionary="MedDRA" Version="26.1"', xml, fixed = TRUE),
    "AEDECOD's method" = grepl('OID="MT.AE.AEDECOD"', xml, fixed = TRUE),
    "AEDECOD's ItemRef" = grepl('ItemOID="IT.AE.AEDECOD" OrderNumber="6" Mandatory="Yes" Role="Synonym Qualifier"', xml, fixed = TRUE))
  else {
    # A column's ItemDef, its lines trimmed and joined.
    item <- function(column) {
      begin <- grep(paste0('<ItemDef OID="IT.VS.', column, '"'), lines, fixed = TRUE)
      end <- begin - 1L + match("</ItemDef>", trimws(lines[begin:length(lines)]))
      paste(trimws(lines[begin:end]), collapse = "")
    }
    # The investigator collects the result and its date-time, and under Other
    # each references the annotated form with no page. EPOCH is a permissible
    # timing variable the sponsor derives from the subject's elements.
    collected <- '<def:Origin Type="Collected" Source="Investigator"><def:DocumentRef leafID="LF.acrf"/></def:Origin>'
    stopifnot_case(case$name, "VSDTC is a partial datetime" = grepl('Name="VSDTC" DataType="partialDatetime"', xml, fixed = TRUE),
      "the EPOCH codelist's value" = grepl('CodedValue="FOLLOW-UP"', xml, fixed = TRUE),
      "VSORRES's ItemRef" =
        grepl('<ItemRef ItemOID="IT.VS.VSORRES" OrderNumber="6" Mandatory="No" Role="Result Qualifier"/>', xml, fixed = TRUE),
      "VSDTC's ItemRef" = grepl('<ItemRef ItemOID="IT.VS.VSDTC" OrderNumber="7" Mandatory="No" Role="Timing"/>', xml, fixed = TRUE),
      "EPOCH's ItemRef" =
        grepl('<ItemRef ItemOID="IT.VS.EPOCH" OrderNumber="8" Mandatory="No" Role="Timing" MethodOID="MT.VS.EPOCH"/>', xml, fixed = TRUE),
      "VSORRES is collected" = grepl(collected, item("VSORRES"), fixed = TRUE),
      "VSDTC is collected" = grepl(collected, item("VSDTC"), fixed = TRUE),
      "EPOCH is derived" = grepl('<def:Origin Type="Derived" Source="Sponsor"/>', item("EPOCH"), fixed = TRUE))
  }
  unlink(root, recursive = TRUE)
}

root <- fixture("sdtm-dm-metadata")
result <- submission(root)
stopifnot("an unpublished dataset breaks its producer contract" = "producer_contract_mismatch" %in% result$issues$condition,
  "under INPUT-11" = "INPUT-11" %in% result$issues$rule, "its save is refused" = refused(result),
  "no Dataset-JSON is written" = !file.exists(file.path(root, "dm.json")),
  "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
# A document beside the published tables, not a submission (DEFINE-4).
metadata_only(root)
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) append(lines, "  context: Other", after = match("sdtm:", lines)))
result <- submission(root); check_issues(result)
edit_lines(project, function(lines) gsub("name: SDTMIG", "name: ADaMIG", lines, fixed = TRUE))
result <- submission(root)
stopifnot("ADaMIG in the SDTM section is a model mismatch" = "standard_model_mismatch" %in% result$issues$condition,
  "its save is refused" = refused(result), "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
result <- submission(root); check_issues(result)
dir.create(file.path(root, "define.xml"))
error <- tryCatch(result$save(), yamaa_error = identity)
stopifnot("a save over a directory signals yamaa_error" = inherits(error, "yamaa_error"),
  "the error carries the submission's issues" = identical(error$issues, result$issues),
  "no Dataset-JSON is written" = !file.exists(file.path(root, "dm.json")),
  "the directory stays" = dir.exists(file.path(root, "define.xml")))
expect_identical(as.list(result$issues[c("phase", "condition", "rule", "spec_paths", "context")]),
  list(phase = "output", condition = "resource_path_not_regular_file", rule = "FILE-7",
    spec_paths = list("sdtm.define"), context = '{"path":"define.xml"}'),
  "a Define-XML path that is a directory")
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) lines[!grepl("source_system_version:", lines, fixed = TRUE)])
result <- submission(root); check_issues(result)
version <- yamaa:::.core_version()
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot("Define-XML names the engine's version by default" =
    grepl(paste0('SourceSystemVersion="', version, '"'), rawToChar(held[["define.xml"]]), fixed = TRUE),
  "Dataset-JSON names the engine's version by default" =
    grepl(paste0('"version": "', version, '"'), rawToChar(held[["dm.json"]]), fixed = TRUE))
unlink(file.path(root, names(held)))
# A terminology standard's name is closed where the project is read (CT-2),
# before any document is composed.
edit_lines(project, function(lines) gsub("name: CDISC/NCI", "name: InvalidStandard", lines, fixed = TRUE))
result <- submission(root)
expect_identical(as.list(result$issues[c("condition", "rule", "spec_paths")]),
  list(condition = "value_not_permitted", rule = "SCHEMA-12", spec_paths = list("codelists[0].standard.name")),
  "an unknown terminology standard")
stopifnot("its save is refused" = refused(result), "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
# A SAS format name longer than eight characters fails only against the
# Define-XML schema's facet, as the document is generated.
edit_lines(project, function(lines) gsub("format_name: $SEX", "format_name: $SEXCODELIST",
  gsub("name: InvalidStandard", "name: CDISC/NCI", lines, fixed = TRUE), fixed = TRUE))
result <- submission(root)
stopifnot("a long format name fails the schema facet" = "DEFINE-25" %in% result$issues$rule,
  "its save is refused" = refused(result), "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
project <- file.path(root, "project.yaml")
lines <- readLines(project)
metadata <- which(lines == "  metadata_version:")
stopifnot("the project declares one metadata version" = length(metadata) == 1L)
lines <- lines[-(metadata + 0:3)]
lines <- lines[!grepl("^  file_oid:", lines)]
writeLines(c(lines, "adam:", "  standards: [{name: ADaMIG, version: '1.3', status: Final}]",
  "  specs: [missing-other-section.yaml]", "  define: adam.xml"), project)
result <- submission(root); check_issues(result)
xml <- rawToChar(saved(result, root, c("dm.json", "define.xml"))[["define.xml"]])
stopifnot("the default file OID" = grepl('FileOID="STUDY01.sdtm.Define-XML_2.1"', xml, fixed = TRUE),
  "the default metadata version" = grepl('OID="MDV.STUDY01.sdtm" Name="STUDY01 sdtm metadata"', xml, fixed = TRUE),
  "the other section is not composed" = !file.exists(file.path(root, "adam.xml")))
unlink(file.path(root, c("dm.json", "define.xml")))
lines <- readLines(project)
acrf <- which(lines == "  - id: acrf"); stopifnot("the project declares the aCRF once" = length(acrf) == 1L)
writeLines(lines[-(acrf + 0:3)], project)
result <- submission(root)
stopifnot("an origin's missing document" = "origin_document_missing" %in% result$issues$condition,
  "its save is refused" = refused(result), "no Dataset-JSON is written" = !file.exists(file.path(root, "dm.json")))
writeLines(lines[lines != "    alias: C66734"], project)
result <- submission(root)
stopifnot("a codelist without its alias fails DEFINE-4" = "DEFINE-4" %in% result$issues$rule,
  "its save is refused" = refused(result), "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata")
for (name in c("parents", "children", "artifacts")) dir.create(file.path(root, name))
spec <- readLines(file.path(root, "spec.yaml"))
spec <- gsub("path: dm.csv", "path: ../artifacts/dm.csv", spec, fixed = TRUE)
writeLines(spec, file.path(root, "parents/base.yaml"))
writeLines(c('yamaa: "1.0"', "type: spec", "parent: ../parents/base.yaml"), file.path(root, "children/spec.yaml"))
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) gsub("define: define.xml", "define: artifacts/define.xml",
  gsub("  - spec.yaml", "  - children/spec.yaml", lines, fixed = TRUE), fixed = TRUE))
stopifnot("the published dataset is copied" =
  file.copy(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"), file.path(root, "artifacts/dm.csv")))
result <- submission(root); check_issues(result)
held <- saved(result, file.path(root, "artifacts"), c("dm.json", "define.xml"))
stopifnot("Define-XML links the Dataset-JSON beside it" = grepl('xlink:href="dm.json"', rawToChar(held[["define.xml"]]), fixed = TRUE),
  "Dataset-JSON references the Define-XML beside it" =
    grepl('"metaDataRef": "define.xml"', rawToChar(held[["dm.json"]]), fixed = TRUE),
  "nothing is written beside the project" = !file.exists(file.path(root, "dm.json")))
# The SEX codelist lists its items, so an allowed_values beside it states the same value set twice.
sex <- which(spec == "    derivation: DM_RAW.SEX"); stopifnot("the base derives SEX once" = length(sex) == 1L)
writeLines(append(spec, c("    verifications:", "      - allowed_values:", "          values: [F, M, U]"), after = sex),
  file.path(root, "parents/base.yaml"))
result <- submission(root)
stopifnot("allowed values beside a codelist are redundant" = "redundant_declaration" %in% result$issues$condition,
  "under CT-9" = "CT-9" %in% result$issues$rule, "its save is refused" = refused(result))
expect_identical(rawfile(file.path(root, "artifacts/define.xml")), held[["define.xml"]],
  "a refused save leaves the earlier Define-XML")
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata")
dir.create(file.path(root, "artifacts"))
spec_path <- file.path(root, "spec.yaml"); spec <- readLines(spec_path)
writeLines(gsub("path: dm.csv", "path: artifacts/dm.csv", spec, fixed = TRUE), spec_path)
stopifnot("the published dataset is copied" =
  file.copy(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"), file.path(root, "artifacts/dm.csv")))
# The transport file sits beside the document wherever the artifact is.
result <- submission(root); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot("Define-XML links the Dataset-JSON beside it" = grepl('xlink:href="dm.json"', rawToChar(held[["define.xml"]]), fixed = TRUE),
  "nothing is written beside the artifact" = !file.exists(file.path(root, "artifacts/dm.json")))
unlink(file.path(root, c("dm.json", "define.xml")))
writeLines(spec, spec_path)
csv <- readLines(file.path(fixtures, "sdtm-dm-metadata/expected/dm.csv"))
writeLines(gsub("STUDYID,USUBJID", "USUBJID,STUDYID", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
stopifnot("reordered columns break the producer contract" = "INPUT-11" %in% result$issues$rule,
  "its save is refused" = refused(result))
writeLines(gsub("STUDYID", "WRONG", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
stopifnot("a renamed column breaks the producer contract" = "INPUT-11" %in% result$issues$rule,
  "its save is refused" = refused(result), "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
writeLines(gsub(",F,USA", ",Z,USA", csv, fixed = TRUE), file.path(root, "dm.csv"))
result <- submission(root)
rows <- result$issues[result$issues$rule %in% "CT-7", , drop = FALSE]
stopifnot("one value is outside its codelist" = nrow(rows) == 1L,
  "the codelist is verified" = identical(rows$phase, "verification"),
  "the finding names the value" = grepl('"value":"Z"', rows$context, fixed = TRUE),
  "the finding names the subject" = grepl("CATH-UCSD-0001", rows$context, fixed = TRUE),
  "its save is refused" = refused(result), "no Dataset-JSON is written" = !file.exists(file.path(root, "dm.json")))
unlink(root, recursive = TRUE)

root <- fixture("sdtm-dm-metadata", TRUE)
spec_path <- file.path(root, "spec.yaml")
spec <- paste(readLines(spec_path), collapse = "\n")
spec <- gsub("name: AGE\n    type: int", "name: AGE\n    type: float", spec, fixed = TRUE)
spec <- gsub(host_truth[["numeric_original_metadata"]], host_truth[["numeric_completion_metadata"]], spec, fixed = TRUE)
writeLines(spec, spec_path)
project <- file.path(root, "project.yaml")
source <- paste(readLines(project), collapse = "\n")
source <- gsub("study:\n", host_truth[["numeric_codelist_yaml"]], source, fixed = TRUE)
writeLines(source, project)
result <- submission(root); check_issues(result)
held <- saved(result, root, c("dm.json", "define.xml"))
stopifnot("a float AGE is a float column" =
  grepl('"name": "AGE", "label": "Age", "dataType": "float"', rawToChar(held[["dm.json"]]), fixed = TRUE))
unlink(root, recursive = TRUE)
root <- fixture("sdtm-dm-metadata", TRUE)
sentinel <- file.path(root, "dm.json")
writeBin(charToRaw("original publication sentinel"), sentinel)
# Under Other a lower-case dataset name is admitted, and its transport file is
# named for it in lower case beside the document.
project <- file.path(root, "project.yaml")
edit_lines(project, function(lines) sub("^sdtm:$", "sdtm:\n  context: Other",
  gsub("  - spec.yaml", "  - spec.yaml\n  - second.yaml", lines, fixed = TRUE)))
spec <- gsub("domain: DM", "domain: dm", readLines(file.path(root, "spec.yaml")), fixed = TRUE)
writeLines(gsub("path: dm.csv", "path: second.csv", spec, fixed = TRUE), file.path(root, "second.yaml"))
define_sentinel <- file.path(root, "define.xml")
writeBin(charToRaw("original Define sentinel"), define_sentinel)
previous_names <- sort(list.files(root, all.files = TRUE, no.. = TRUE))
result <- submission(root)
# The shared truth's row, which Python compares too. Its context names the two
# files by their absolute paths, so it is compared by the file name both
# writers would write.
truth <- host_truth[["transport_name_collision_issue"]]
columns <- issue_columns_of(result$issues)
located <- setdiff(names(columns), "context")
expect_identical(columns[located],
  issue_truth(list(truth), normalizePath(root, winslash = "/", mustWork = TRUE))[located],
  "two datasets named dm in any case")
stopifnot("the finding names the colliding file" = grepl(truth$context_names, result$issues$context, fixed = TRUE),
  "its save is refused" = refused(result))
expect_identical(rawfile(sentinel), charToRaw("original publication sentinel"), "a collision leaves the Dataset-JSON")
expect_identical(rawfile(define_sentinel), charToRaw("original Define sentinel"), "a collision leaves the Define-XML")
expect_identical(sort(list.files(root, all.files = TRUE, no.. = TRUE)), previous_names, "a collision writes nothing")
unlink(root, recursive = TRUE)
for (case_index in seq_along(host_truth[["xml_text_cases"]])) {
  case <- host_truth[["xml_text_cases"]][[case_index]]
  label <- paste("XML text case", case_index)
  root <- fixture("sdtm-dm-metadata")
  metadata_only(root)
  for (filename in c("spec.yaml", "project.yaml")) {
    path <- file.path(root, filename)
    text <- paste(readLines(path), collapse = "\n")
    mutations <- if (filename == "spec.yaml") case[["specification_mutations"]] else case[["project_mutations"]]
    for (mutation in mutations) {
      stopifnot_case(paste(label, filename), "the mutation's text is present" = grepl(mutation[[1L]], text, fixed = TRUE))
      text <- sub(mutation[[1L]], mutation[[2L]], text, fixed = TRUE)
    }
    writeLines(text, path)
  }
  if (case[["inherit_specification"]]) {
    stopifnot_case(label, "the specification becomes a parent" = file.rename(file.path(root, "spec.yaml"), file.path(root, "base.yaml")))
    writeLines(c('yamaa: "1.0"', "type: spec", "parent: base.yaml"), file.path(root, "spec.yaml"))
  }
  result <- submission(root)
  check_issues(result, empty = !length(case[["issues"]]))
  stopifnot_case(label, "composing writes no Define-XML" = !file.exists(file.path(root, "define.xml")),
    "composing writes no Dataset-JSON" = !file.exists(file.path(root, "dm.json")))
  expect_issue_rows(result$issues, case[["issues"]], label)
  if (length(case[["issues"]])) {
    stopifnot_case(label, "its save is refused" = refused(result),
      "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
  } else {
    xml <- rawToChar(saved(result, root, "define.xml")[["define.xml"]])
    for (fragment in case[["xml_fragments"]]) stopifnot_case(label, "Define-XML holds the authored fragment" =
      grepl(fragment, xml, fixed = TRUE))
  }
  stopifnot_case(label, "no input directory is created" = !dir.exists(file.path(root, "input")),
    "metadata alone reads no dataset" = !file.exists(file.path(root, "dm.csv")))
  unlink(root, recursive = TRUE)
}
# Every benchmark declaring a Define call: positive cases hold and save the
# expected bytes; negative cases fail with the expected first issue.
manifest <- jsonlite::fromJSON(file.path(fixtures, "benchmarks", "manifest.json"), simplifyVector = FALSE)
case_names <- vapply(manifest$cases, `[[`, character(1), "name")
stopifnot("the manifest lists the transport and metadata cases" =
  all(c("sdtm-dm-xpt", "adam-adsl-xpt", "sdtm-dm-metadata") %in% case_names))
for (case in manifest$cases) {
  source <- file.path(fixtures, "benchmarks", case$name)
  root <- tempfile(paste0(case$name, "-")); dir.create(root)
  files <- list.files(source)
  files <- files[!dir.exists(file.path(source, files))]
  stopifnot_case(case$name, "the benchmark's files are copied" = all(file.copy(file.path(source, files), root)))
  for (name in unlist(case$published)) stopifnot_case(case$name, "a published dataset is copied" =
    file.copy(file.path(source, "expected", name), file.path(root, name)))
  result <- yamaa_project(file.path(root, case$project))$submission(case$standard, created = case$created)
  if (!is.null(case$error)) {
    want <- case$error
    stopifnot_case(case$name, "the submission has issues" = nrow(result$issues) >= 1L)
    expect_identical(list(result$issues$phase[[1L]], result$issues$condition[[1L]], result$issues$rule[[1L]],
        result$issues$spec_paths[[1L]]),
      list(want$phase, want$condition, want$rule, fixture_strings(want$spec_paths)),
      paste(case$name, "fails with the expected first issue"))
    context <- jsonlite::fromJSON(result$issues$context[[1L]], simplifyVector = FALSE)
    # A partial error.yaml may leave the context out; it states only the keys
    # it compares, as Python's suite reads it too.
    if (!is.null(want$context))
      expect_identical(context[names(want$context)], want$context, paste(case$name, "states the expected context"))
    stopifnot_case(case$name, "its save is refused" = refused(result),
      "no Define-XML is written" = !file.exists(file.path(root, "define.xml")))
  } else {
    check_issues(result)
    held <- saved(result, root, unlist(case$expected))
    for (name in names(held)) expect_identical(held[[name]], rawfile(file.path(source, "expected", name)),
      paste(case$name, "saves the expected", name))
  }
  unlink(root, recursive = TRUE)
}
restore_path()
cat("PASS: shared installed R submission DM goldens, AE/VS structure, issues, held save, version and zero-write failures\n")
