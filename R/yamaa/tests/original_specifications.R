# Build tools may include Python, but the installed runtime probe must not see it.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("native-only-path-")
dir.create(runtime_path)
# R itself may invoke rm during temporary-file cleanup; preserve only that utility.
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"context.R"),local=TRUE)
local({
  decode <- function(text) jsonlite::fromJSON(text,simplifyVector=FALSE)
  actual <- decode('{"keys":[{"B":2,"A":1},null],"dataset":"SRC"}')
  stopifnot(context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":1,"B":2},null]}')),
    !context_equal(actual,decode('{"dataset":"SRC","keys":[null,{"A":1,"B":2}]}')),
    !context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":"1","B":2},null]}')),
    !context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":1,"B":2}]}')))
})
host_truth <- read_host_fixture(host_fixture_root,"original-specifications")
root <- system.file("specification-original", package="yamaa", mustWork=TRUE)
fixture_rows <- function(filename) {
  jsonlite::fromJSON(file.path(root,filename),simplifyVector=FALSE)
}
json_equal <- function(actual,expected) {
  wide_number <- function(value) {
    if(is.numeric(value)) return(any(abs(value)>2^53-1))
    is.list(value) && any(vapply(value,wide_number,logical(1)))
  }
  actual <- jsonlite::fromJSON(actual,simplifyVector=FALSE)
  if(wide_number(actual) || wide_number(expected)) {
    stop("wide JSON numbers require an independently authored wire assertion")
  }
  context_equal(actual,expected)
}
r_report <- function(expected) {
  if(is.character(expected)) {
    expected <- gsub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
    expected <- gsub('"runtime_version":"fixture-runtime"',paste0('"runtime_version":"',as.character(getRversion()),'"'),expected,fixed=TRUE)
    return(gsub('"engine_version":"fixture-engine"',paste0('"engine_version":"',yamaa:::.core_version(),'"'),expected,fixed=TRUE))
  }
  expected$runtime <- "r"
  expected$runtime_version <- as.character(getRversion())
  expected$engine_version <- yamaa:::.core_version()
  expected
}
unsaved_report <- function(expected) {
  if(is.character(expected)) return(sub('"artifacts":\\[.*?\\],"backend"','"artifacts":[],"backend"',expected,perl=TRUE))
  expected$artifacts <- list()
  expected
}
report_equal <- function(actual,expected) {
  if(is.character(expected)) identical(actual,expected) else json_equal(actual,expected)
}
local({
  wire <- '{"minimum":-9223372036854775808,"maximum":9223372036854775807}'
  adjacent <- sub("9223372036854775807","9223372036854775806",wire,fixed=TRUE)
  stopifnot(!identical(wire,adjacent),
    inherits(tryCatch(json_equal(wire,jsonlite::fromJSON(adjacent,simplifyVector=FALSE)),
      error=identity),"error"))
})
source(file.path(root,"issue-frame-truth.R"),local=TRUE)
check_issue_frame <- function(actual,expected) {
  stopifnot(is.data.frame(actual),identical(names(actual),fixture_strings(host_truth[["issue_columns"]])))
  stopifnot(nrow(actual)==length(expected$phase),identical(lapply(actual,identity),expected))
  stopifnot(all(vapply(actual[c("phase","condition","requirement","context")],is.character,logical(1))),is.list(actual$spec_paths))
}
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
# Public calls retain the complete independently authored original reports.
public_expected <- function(expected, entry) {
  if (is.character(expected)) expected <- jsonlite::fromJSON(expected, simplifyVector=FALSE)
  expected <- r_report(expected)
  expected$example <- "domain"
  for (group in c("nodes", "tables", "verifications", "callbacks")) {
    for (index in seq_along(expected[[group]])) expected[[group]][[index]]$specification <- entry
  }
  expected
}
public_observed <- function(result) {
  jsonlite::fromJSON(rawToChar(get(".domain_observations",asNamespace("yamaa"))(result)),simplifyVector=FALSE)
}
source(file.path(root,"public-output-truth.R"),local=TRUE)
domain_observations <- get(".domain_observations",envir=asNamespace("yamaa"))
json_text <- function(value) {
  # Quote only run-identity labels; original semantic truth remains unchanged.
  characters <- strsplit(enc2utf8(value),"",fixed=TRUE)[[1L]]
  escaped <- vapply(characters,function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code),
      "8"="\\b","9"="\\t","10"="\\n","12"="\\f","13"="\\r",
      "34"="\\\"","92"="\\\\",
      if(code<32L) sprintf("\\u%04x",code) else character)
  },character(1L),USE.NAMES=FALSE)
  paste0('"',paste0(escaped,collapse=""),'"')
}
for(case_name in names(issue_frame_truth)[!startsWith(names(issue_frame_truth),"static/")]) {
  directory <- tempfile("public-domain-");dir.create(directory)
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml")
  built <- yamaa_domain(entry)
  report <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  report <- sub('"runtime":"python"','"runtime":"r"',report,fixed=TRUE)
  report <- sub('fixture-runtime',as.character(getRversion()),report,fixed=TRUE)
  report <- sub('fixture-engine',yamaa:::.core_version(),report,fixed=TRUE)
  report <- sub(paste0('"example":"',case_name,'"'),'"example":"domain"',report,fixed=TRUE)
  entry_field <- paste0('"specification":',json_text(entry))
  report <- gsub(paste0('"specification":',json_text(basename(entry))),
                 entry_field,report,fixed=TRUE)
  unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',report)
  observed <- rawToChar(domain_observations(built))
  if(!identical(observed,unsaved)) {
    cat(case_name,"public unsaved report mismatch\nexpected:\n",unsaved,
        "\nactual:\n",observed,"\n",sep="")
  }
  stopifnot(identical(observed,unsaved))
  check_issue_frame(built$issues,issue_frame_truth[[case_name]])
  stopifnot(is.null(built$verification_log),is.null(built$warning_log))
  expected <- public_output_truth[[case_name]]
  if(is.null(expected)) {
    stopifnot(is.null(built$output),inherits(tryCatch(built$save(),error=identity),"yamaa_domain_error"))
    stopifnot(identical(rawToChar(domain_observations(built)),unsaved))
  } else {
    stopifnot(is.data.frame(built$output),identical(as.list(built$output),expected))
    output_name <- switch(case_name,"schema-lookup"="adsl.csv","schema-window-functions"="advs.csv","adlb.csv")
    for(attempt in seq_len(2L)) {
      stopifnot(identical(built$save(),TRUE),identical(rawfile(file.path(case,output_name)),rawfile(file.path(case,"expected",output_name))))
      stopifnot(identical(rawToChar(domain_observations(built)),report))
    }
    stopifnot(identical(as.list(built$output),expected))
  }
  report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  if(nzchar(report_directory)) {
    # Relabel only the observed run identity; semantic findings and reads stay exact.
    observed <- rawToChar(domain_observations(built))
    observed <- sub('"example":"domain"',paste0('"example":"',case_name,'"'),observed,fixed=TRUE)
    observed <- gsub(entry_field,
                     paste0('"specification":',json_text(basename(entry))),observed,fixed=TRUE)
    dir.create(report_directory,recursive=TRUE,showWarnings=FALSE)
    writeBin(charToRaw(observed),file.path(report_directory,paste0(case_name,".r.rust.json")))
  }
  unlink(directory,recursive=TRUE)
  cat(case_name,"public domain typed output, complete issues and exact saved bytes passed\n")
  cat(case_name,"public domain complete Rust run report and successful save observations passed\n")
}
directory <- tempfile("public-check-");dir.create(directory)
entry <- file.path(directory,"spec.yaml")
writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
             "output: {path: absent/output.csv, columns: [ID]}","columns:",
             "  - {name: ID, type: int, derivation: {source: SRC.ID}}"),entry)
stopifnot(nrow(yamaa_check(entry)$issues)==0L)
failed <- yamaa_domain(entry)
stopifnot(is.null(failed$output),identical(failed$issues$condition,"resource_path_missing"),inherits(tryCatch(failed$save(),error=identity),"yamaa_domain_error"))
writeBin(fixture_bytes(host_truth[["integer_boundary_csv"]]),file.path(directory,"input.csv"))
built <- yamaa_domain(entry)
stopifnot(identical(as.character(built$output$ID),c("-9223372036854775808","9223372036854775807")),identical(is.na(built$output$ID),c(FALSE,FALSE)))
writeLines("[changed",entry); writeLines("changed",file.path(directory,"input.csv"))
stopifnot(identical(built$save(),FALSE),identical(built$issues$condition,"engine_rejected"))
dir.create(file.path(directory,"absent"))
stopifnot(identical(built$save(),TRUE),nrow(built$issues)==0L,identical(rawfile(file.path(directory,"absent/output.csv")),fixture_bytes(host_truth[["integer_boundary_csv"]])))
refused <- yamaa_domain("absent.yaml",environment="absent-environment.yaml")
stopifnot(is.null(domain_observations(refused)))
stopifnot(is.null(refused$output),identical(refused$issues$phase,"validation"),
  identical(refused$issues$condition,"project_environment_missing"),
  identical(refused$issues$requirement,"REQ-0694"),identical(refused$issues$spec_paths,list("environment")),
  identical(refused$issues$context,paste0('{"cause":"missing","path":',
    json_text(file.path(normalizePath(getwd(),winslash="/"),"absent-environment.yaml")),'}')),
  identical(refused$issues,yamaa_check("absent.yaml",environment="absent-environment.yaml")$issues))
stopifnot(inherits(tryCatch(yamaa_domain(NULL),error=identity),"error"))
unlink(directory,recursive=TRUE)
cat("public check without study data, full-range integers, retained save retry and environment refusal passed\n")

remove_directory_link <- function(path) {
  if(.Platform$OS.type=="windows") {
    # R's unlink treats directory symbolic links as mount-point reparse tags.
    # rmdir without /s removes only this link, preserving its external target.
    command <- Sys.getenv("COMSPEC")
    stopifnot(nzchar(command),file.exists(command))
    system2(command,c("/d","/c","rmdir",shQuote(chartr("/","\\",path),type="cmd")))
  } else unlink(path,recursive=TRUE)
}
for(late in c(FALSE,TRUE)) for(existing in c(FALSE,TRUE)) {
  directory <- tempfile("public-linked-output-");dir.create(directory)
  outside <- tempfile("public-output-outside-");dir.create(outside)
  sentinel <- file.path(outside,"sentinel")
  sentinel_bytes <- charToRaw("outside sentinel")
  writeBin(sentinel_bytes,sentinel)
  entry <- file.path(directory,"spec.yaml")
  writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
               "output: {path: link/output.csv, columns: [ID]}","columns:",
               "  - {name: ID, type: int, derivation: {source: SRC.ID}}"),entry)
  writeBin(charToRaw("ID\n1\n"),file.path(directory,"input.csv"))
  target <- file.path(outside,"output.csv")
  if(existing) writeBin(charToRaw("retained"),target)
  link <- file.path(directory,"link")
  if(late) dir.create(link) else stopifnot(file.symlink(outside,link))
  built <- yamaa_domain(entry)
  stopifnot(identical(as.character(built$output$ID),"1"))
  if(late) {
    stopifnot(identical(unlink(link,recursive=TRUE),0L),!file.exists(link),file.symlink(outside,link))
  }
  stopifnot(identical(built$save(),FALSE),identical(built$issues$context,'{"code":"invalid_target","stage":"output"}'),
            identical(list.files(outside,all.files=TRUE,no..=TRUE),if(existing) c("output.csv","sentinel") else "sentinel"))
  stopifnot(identical(rawfile(sentinel),sentinel_bytes))
  if(existing) stopifnot(identical(rawfile(target),charToRaw("retained")))
  stopifnot(identical(remove_directory_link(link),0L),!file.exists(link),
            identical(rawfile(sentinel),sentinel_bytes))
  if(existing) stopifnot(identical(rawfile(target),charToRaw("retained")))
  stopifnot(dir.create(link))
  stopifnot(identical(built$save(),TRUE),nrow(built$issues)==0L,identical(rawfile(file.path(link,"output.csv")),charToRaw("ID\n1\n")))
  stopifnot(identical(rawfile(sentinel),sentinel_bytes))
  unlink(directory,recursive=TRUE);unlink(outside,recursive=TRUE)
}
cat("public save rejects linked parents before or after build, retains external targets and permits safe retry passed\n")


local({
  directory <- tempfile("public-parquet-original-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  original <- file.path(root,"cases","adam-adlb-ordered-sum")
  for (input_parquet in c(FALSE,TRUE)) for (output_parquet in c(FALSE,TRUE)) {
    work <- file.path(directory,paste(input_parquet,output_parquet,sep="-")); dir.create(work)
    dir.create(file.path(work,"input"))
    source <- rawToChar(rawfile(file.path(original,"spec.yaml")))
    if (input_parquet) {
      source <- sub(host_truth[["floating_input_override"]],"path: input/lb.parquet",source,fixed=TRUE)
      stopifnot(file.copy(file.path(root,"pq","ordered-sum.parquet"),file.path(work,"input","lb.parquet")))
    } else stopifnot(file.copy(file.path(original,"input","lb.csv"),file.path(work,"input","lb.csv")))
    if (output_parquet) source <- sub("path: adlb.csv","path: adlb.parquet",source,fixed=TRUE)
    entry <- file.path(work,"spec.yaml"); writeBin(charToRaw(source),entry)
    entry <- normalizePath(entry,winslash="/",mustWork=TRUE)
    expected <- public_expected(rawToChar(rawfile(file.path(root,"expected","adam-adlb-ordered-sum.json"))),entry)
    if (input_parquet) for (index in seq_along(expected$source_reads)) expected$source_reads[[index]]$path <- "input/lb.parquet"
    result <- yamaa_domain(entry)
    stopifnot(nrow(result$issues)==0L,!is.null(result$output),context_equal(public_observed(result),unsaved_report(expected)))
    unlink(entry); unlink(file.path(work,"input"),recursive=TRUE)
    for (attempt in seq_len(2L)) {
      stopifnot(identical(result$save(),TRUE))
      observed <- public_observed(result)
      stopifnot(context_equal(unsaved_report(observed),unsaved_report(expected)))
      if (output_parquet) {
        bytes <- rawfile(file.path(work,"adlb.parquet"))
        artifact <- observed$artifacts[[1L]]
        stopifnot(identical(rawToChar(bytes[1:4]),"PAR1"),identical(rawToChar(tail(bytes,4L)),"PAR1"),
          identical(artifact$profile,"parquet"),artifact$byte_length==length(bytes),
          length(artifact$records)==artifact$row_count+1L)
      } else stopifnot(identical(rawfile(file.path(work,"adlb.csv")),rawfile(file.path(original,"expected","adlb.csv"))),
        context_equal(observed,expected))
    }
  }
  cat("public R Parquet file input/output preserve complete original reports and held saves without Python\n")
})

local({
  directory <- tempfile("public-parquet-refusals-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  entry <- file.path(directory,"spec.yaml")
  source <- '{"schema_version":"1.0","domain":"TEST","base":"SRC","input":{"SRC":{"path":"input.parquet"}},"keys":["ID"],"columns":[{"name":"ID","type":"int","derivation":{"compute":{"expr":"1"}}}],"output":{"path":"output.csv","columns":["ID"]}}'
  writeBin(charToRaw(source),entry)
  entry <- normalizePath(entry,winslash="/",mustWork=TRUE)
cases <- list(
  c("utf8","source_parquet_invalid","REQ-1038",'"dataset":"SRC","path":"input.parquet"'),
  c("utf8-bool","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"OTHER","path":"input.parquet","stored_type":"bool"'),
  c("utf8-time","source_field_value_invalid","REQ-1041",'"dataset":"SRC","field":"OTHER","path":"input.parquet","row":1,"value":1'),
  c("empty-name","source_field_name_empty","REQ-1039",'"dataset":"SRC","field":1,"path":"input.parquet"'),
  c("duplicate","source_field_name_duplicate","REQ-1039",'"dataset":"SRC","field":"I","path":"input.parquet"'),
  c("mixed-struct","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"S","path":"input.parquet","stored_type":"struct<left: int64, right: string>"'),
  c("mixed-map","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"M","path":"input.parquet","stored_type":"map<string, int64 (\'M\')>"'),
  c("mixed-empty","source_field_name_empty","REQ-1039",'"dataset":"SRC","field":1,"path":"input.parquet"'),
  c("mixed-duplicate","source_field_name_duplicate","REQ-1039",'"dataset":"SRC","field":"I","path":"input.parquet"')
)
unsupported_types <- c(int32="int32",uint64="uint64",float32="float",binary="binary",
  milliseconds="timestamp[ms]",timezone="timestamp[us, tz=UTC]",list="list<element: int64>",
  "fixed-list"="fixed_size_list<element: int64>[2]",struct="struct<item: int64>",decimal="decimal128(10, 2)")
for(name in names(unsupported_types)) cases[[length(cases)+1L]] <- c(
  paste0("unsupported-",name),"source_field_type_unsupported","REQ-1040",
  paste0('"dataset":"SRC","field":"FIELD","path":"input.parquet","stored_type":"',unsupported_types[[name]],'"'))
for (test in cases) {
  stopifnot(file.copy(file.path(root,"pq",paste0(test[[1L]],".parquet")),file.path(directory,"input.parquet"),overwrite=TRUE))
  result <- yamaa_domain(entry)
  finding <- paste0('{"condition":"',test[[2L]],'","context":{',test[[4L]],'},"phase":"ingest","requirement":"',test[[3L]],'","spec_paths":["input.SRC.path"]}')
  expected <- paste0('{"artifacts":[],"backend":"rust","callbacks":[],"diagnostics":[',finding,'],"engine_version":"',yamaa:::.core_version(),'","error":null,"example":"',test[[1L]],'","handler_counts":[],"nodes":[{"diagnostics":[',finding,'],"handler_counts":[],"outcome":"failure","specification":"spec.yaml","unsupported":[]}],"outcome":"failure","report_version":"0.3.0-draft","runtime":"r","runtime_version":"',as.character(getRversion()),'","source_reads":[{"base_directory":".","condition":null,"outcome":"captured","path":"input.parquet","snapshots_created":1}],"tables":[],"unsupported":[],"verifications":[]}')
  expected <- public_expected(expected,entry)
  stopifnot(is.null(result$output),context_equal(public_observed(result),expected),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
    !file.exists(file.path(directory,"output.csv")))
}
  cat("public R Parquet closed types and exact complete refusal reports passed without Python\n")
})

local({
  directory <- tempfile("public-parquet-empty-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for (policy in c("missing","present")) {
    entry <- file.path(directory,"spec.yaml")
    source <- paste0('{"schema_version":"1.0","domain":"TEST","input":{"SRC":{"path":"input.PARQUET","empty_string":"',policy,'"}},"keys":["I"],"columns":[{"name":"I","type":"int","derivation":"SRC.I"},{"name":"S","type":"str","derivation":"SRC.S"}],"output":{"path":"output.csv","columns":["I","S"]}}')
    writeBin(charToRaw(source),entry)
    stopifnot(file.copy(file.path(root,"pq","text.parquet"),file.path(directory,"input.PARQUET"),overwrite=TRUE))
    result <- yamaa_domain(entry)
    stopifnot(nrow(result$issues)==0L)
    expected <- charToRaw(if (policy=="missing") "I,S\n1,\n2,X\n" else 'I,S\n1,""\n2,X\n')
    unlink(entry); unlink(file.path(directory,"input.PARQUET")); invisible(gc())
    for (attempt in seq_len(2L)) stopifnot(identical(result$save(),TRUE),identical(rawfile(file.path(directory,"output.csv")),expected))
  }
  cat("public R Parquet empty-string policy and retained exact saved bytes passed without Python\n")
})

local({
  # arrow writes a factor as a BYTE_ARRAY String field whose embedded Arrow
  # schema names a dictionary. A layout is not a type (REQ-1032), so each
  # field reads as its text (#2940).
  stopifnot(requireNamespace("arrow", quietly = TRUE))
  directory <- tempfile("public-parquet-factors-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  text <- c("b",NA,"","a","b","\u03b2")
  levels <- c("a","b","","\u03b2")
  path <- file.path(directory,"input.parquet")
  arrow::write_parquet(data.frame(ID=as.character(1:6),FACTOR=factor(text,levels=levels),
    ORDERED=factor(text,levels=levels,ordered=TRUE),stringsAsFactors=FALSE),path)
  stored <- arrow::read_parquet(path,as_data_frame=FALSE)$schema
  stopifnot(inherits(stored$FACTOR$type,"DictionaryType"),inherits(stored$ORDERED$type,"DictionaryType"))
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw('{"schema_version":"1.0","domain":"TEST","input":{"SRC":{"path":"input.parquet","empty_string":"present"}},"keys":["ID"],"columns":[{"name":"ID","type":"str","derivation":"SRC.ID"},{"name":"FACTOR","type":"str","derivation":"SRC.FACTOR"},{"name":"ORDERED","type":"str","derivation":"SRC.ORDERED"}],"output":{"path":"output.csv","columns":["ID","FACTOR","ORDERED"]}}'),entry)
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$output$ID,as.character(1:6)),
    identical(result$output$FACTOR,text),identical(result$output$ORDERED,text))
  cat("public R Parquet factor columns read as text passed without Python\n")
})

local({
  directory <- tempfile("public-rounding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  case_name <- "schema-round-half-away-from-zero-ties"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry)
  input <- file.path(case,"input","adsl.csv")
  captured <- rawfile(input)
  stopifnot(unlink(input)==0L,!file.exists(input))
  stopifnot(nrow(yamaa_check(entry)$issues)==0L)
  writeBin(captured,input)
  result <- yamaa_domain(entry)
  expected <- read.csv(file.path(case,"expected","adsl.csv"),colClasses=c("character","character",rep("numeric",4L)),na.strings="",check.names=FALSE)
  stopifnot(nrow(expected)==16L,nrow(result$issues)==0L,identical(result$output,expected),
            identical(rawfile(entry),written),!file.exists(file.path(case,"adsl.csv")))
  stopifnot(identical(result$save(),TRUE),identical(read.csv(file.path(case,"adsl.csv"),colClasses=c("character","character",rep("numeric",4L)),na.strings="",check.names=FALSE),expected))
  case_name <- "negative-rounding-source"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  input_directory <- file.path(case,"input")
  stopifnot(unlink(input_directory,recursive=TRUE)==0L,!file.exists(input_directory))
  expected_error <- host_truth[["rounding_type_error_yaml"]]
  stopifnot(identical(rawToChar(rawfile(file.path(case,"expected","error.yaml"))),expected_error))
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues
  rejected <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),
    identical(checked$condition,"incompatible_input_type"),identical(checked$requirement,"REQ-0418"),
    identical(checked$spec_paths,list("columns.RNDVAL.derivation.round_half_away_from_zero.source")),
    identical(checked$context,'{"actual":"str","expected":"numeric","source":"SEX"}'),
    identical(rejected$issues,checked),is.null(rejected$output),
    is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),
    !file.exists(file.path(case,"adsl.csv")))
  cat("public original declarative rounding matches 16 committed rows and rejects original nonnumeric input before study capture without Python passed\n")
})

local({
  directory <- tempfile("public-temporal-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("adam-adsl-duration","adam-adsl-analysis-age")) {
    stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
    stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
    result <- yamaa_domain(entry)
    expected <- read.csv(file.path(case,"expected","adsl.csv"),na.strings="",check.names=FALSE)
    dates <- if(case_name=="adam-adsl-duration") c("STDT","ENDT") else c("BRTHDT","RANDDT")
    for(column in dates) expected[[column]] <- as.Date(expected[[column]])
    integers <- if(case_name=="adam-adsl-duration") c("DURW","DURM") else "AAGE"
    for(column in integers) expected[[column]] <- structure(as.character(expected[[column]]),class="yamaa_int64_vector")
    stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
      !file.exists(file.path(case,"adsl.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"adsl.csv")),rawfile(file.path(case,"expected","adsl.csv"))))
  }
  cases <- list(
    list("negative-date-diff-endpoints","columns.SURVDD.derivation.date_diff.start","RFSTDTM","datetime"),
    list("negative-study-day-datetime","columns.DTHDY.derivation.study_day.date","DTHDTM","datetime"),
    list("negative-epoch-day-non-date","columns.EXSTDY.derivation.to_epoch_day.source","EX.EXSTDT","str"),
    list("negative-date-diff-units","columns.TRTDURM.derivation.date_diff.bounds",NULL,NULL))
  for(item in cases) {
    stopifnot(file.copy(file.path(root,"cases",item[[1L]]),directory,recursive=TRUE))
    case <- file.path(directory,item[[1L]]);unlink(file.path(case,"input"),recursive=TRUE)
    bounds <- is.null(item[[3L]])
    condition <- if(bounds) "value_not_permitted" else "incompatible_input_type"
    requirement <- if(bounds) "REQ-0613" else "REQ-0606"
    context_yaml <- if(bounds) "  value: inclusive\n  permitted: [exclusive]\n" else paste0("  source: ",item[[3L]],"\n  expected: date\n  actual: ",item[[4L]],"\n")
    context <- if(bounds) '{"permitted":["exclusive"],"value":"inclusive"}' else paste0('{"actual":"',item[[4L]],'","expected":"date","source":"',item[[3L]],'"}')
    stopifnot(identical(rawToChar(rawfile(file.path(case,"expected","error.yaml"))),paste0("phase: validation\ncondition: ",condition,"\nspec_paths:\n  - ",item[[2L]],"\nrequirement: ",requirement,"\ncontext:\n",context_yaml)))
    checked <- yamaa_check(file.path(case,"spec.yaml"))$issues
    rejected <- yamaa_domain(file.path(case,"spec.yaml"))
    stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,condition),
      identical(checked$requirement,requirement),identical(checked$spec_paths,list(item[[2L]])),
      identical(checked$context,context),identical(rejected$issues,checked),is.null(rejected$output),
      is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
      inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"input")))
  }
  cat("public complete date operations match original independent outputs and four complete static failures without Python passed\n")
})

local({
  directory <- tempfile("public-imputation-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("schema-birth-date","sdtm-dm-age")) {
    stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
    stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
    result <- yamaa_domain(entry)
    output <- if(case_name=="schema-birth-date") "adsl.csv" else "dm.csv"
    expected <- read.csv(file.path(case,"expected",output),colClasses="character",na.strings="",check.names=FALSE)
    dates <- if(case_name=="schema-birth-date") fixture_strings(host_truth[["partial_date_columns"]]) else "RFICDTC"
    for(column in dates) expected[[column]] <- as.Date(expected[[column]])
    if(case_name=="schema-birth-date") expected$DTM <- as.POSIXct(expected$DTM,format="%Y-%m-%dT%H:%M:%S",tz="UTC")
    column <- if(case_name=="schema-birth-date") "DOBDY" else "AGE"
    expected[[column]] <- structure(expected[[column]],class="yamaa_int64_vector")
    stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
      !file.exists(file.path(case,output)),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,output)),rawfile(file.path(case,"expected",output))))
  }
  failures <- read.delim(file.path(root,"temporal-imputation-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  stopifnot(nrow(failures)==10L)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    stopifnot(file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    static <- finding$phase=="validation"
    if(static) unlink(file.path(case,"input"),recursive=TRUE)
    checked <- yamaa_check(entry)$issues
    result <- yamaa_domain(entry);actual <- result$issues
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),
      identical(actual$condition,finding$condition),identical(actual$requirement,finding$requirement),
      identical(actual$spec_paths,list(finding$spec_path)),identical(actual$context,finding$context),
      is.null(result$output),identical(is.null(get(".domain_observations",asNamespace("yamaa"))(result)),static),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),length(list.files(case,pattern="[.]csv$"))==0L)
    if(static) stopifnot(identical(checked,actual)) else stopifnot(nrow(checked)==0L)
  }
  cat("public temporal imputation preserves two original outputs and ten complete failures without Python passed\n")
})

local({
  directory <- tempfile("public-string-case-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  source <- host_truth[["unicode_case_yaml"]]
  entry <- file.path(directory,"spec.yaml");writeBin(charToRaw(source),entry)
  stopifnot(nrow(yamaa_check(entry)$issues)==0L)
  writeBin(charToRaw("ID,TEXT\n1,FOLLOW-UP\n2,\u00df\u0130\u00e9\U0001f642AbC\n3,\n"),file.path(directory,"input.csv"))
  result <- yamaa_domain(entry)
  expected_bytes <- fixture_bytes(host_truth[["unicode_case_csv"]])
  expected <- data.frame(ID=c("1","2","3"),UP=c("FOLLOW-UP","\u00df\u0130\u00e9\U0001f642ABC","MiXeD"),
    LOW=c("follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    SENT=c("Follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    TITLE=c("Follow-Up","\u00df\u0130\u00e9\U0001f642Abc",NA_character_),stringsAsFactors=FALSE)
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),!file.exists(file.path(directory,"output.csv")),
    identical(result$save(),TRUE),identical(rawfile(file.path(directory,"output.csv")),expected_bytes))
  source <- sub("to: upper, missing: MiXeD","to: upper",source,fixed=TRUE)
  writeBin(charToRaw(source),entry);writeBin(charToRaw("ID,TEXT\nrow1,\n"),file.path(directory,"input.csv"))
  rejected <- yamaa_domain(entry);actual <- rejected$issues
  # The complete independent reference lives in text-case-reference.json; this
  # literal is its mapping failure with the independently authored UP path.
  stopifnot(nrow(actual)==1L,identical(actual$phase,"mapping"),identical(actual$condition,"missing_input"),
    identical(actual$requirement,"REQ-0334"),identical(actual$spec_paths,list("columns.UP.derivation.str_case")),
    identical(actual$context,'{"keys":[{"ID":"row1"}],"variable":"SRC.TEXT"}'),is.null(rejected$output),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-case-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  stopifnot(nrow(failures)==2L)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    stopifnot(file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    unlink(file.path(case,"input"),recursive=TRUE)
    checked <- yamaa_check(entry)$issues;rejected <- yamaa_domain(entry);actual <- rejected$issues
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),identical(actual$condition,finding$condition),
      identical(actual$requirement,finding$requirement),identical(actual$spec_paths,list(finding$spec_path)),
      identical(actual$context,finding$context),identical(checked,actual),is.null(rejected$output),
      is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
      inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"input")))
  }
  case_name <- "schema-sentence-case"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  checked <- yamaa_check(entry)$issues;result <- yamaa_domain(entry)
  stopifnot(nrow(checked)==0L,nrow(result$issues)==0L,!is.null(result$output),
    !file.exists(file.path(case,"adlb.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adlb.csv")),rawfile(file.path(case,"expected","adlb.csv"))))
  cat("public ASCII string case matches independent exact outputs, original negatives and original ordered sentence artifact without Python passed\n")
})

local({
  directory <- tempfile("public-source-positions-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  entry <- file.path(directory,"spec.yaml");data <- file.path(directory,"input.csv");output <- file.path(directory,"output.csv")
  writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]",
    "input: {SRC: {path: input.csv, ordinal: REC}}", "filter: \"SRC.KEEP = 'Y'\"",
    "output: {path: output.csv, columns: [ID, POSITION], order_by: [SORT]}", "columns:",
    "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: POSITION, type: int, derivation: SRC.REC}",
    "  - {name: SORT, type: int, derivation: SRC.SORT}"),entry,useBytes=TRUE)
  inputs <- c(host_truth[["filtered_multiline_csv"]],"ID,SORT,KEEP,NOTE\n")
  expected <- c("ID,POSITION\nc,3\nb,2\n","ID,POSITION\n")
  for(i in seq_along(inputs)) {
    writeBin(charToRaw(inputs[[i]]),data)
    stopifnot(nrow(yamaa_check(entry)$issues)==0L)
    result <- yamaa_domain(entry)
    stopifnot(nrow(result$issues)==0L,!is.null(result$output),!file.exists(output))
    if(i==1L) stopifnot(identical(result$output$ID,c("c","b")),identical(as.character(result$output$POSITION),c("3","2")))
    else stopifnot(nrow(result$output)==0L)
    unlink(data)
    for(attempt in seq_len(2L)) stopifnot(identical(result$save(),TRUE),identical(rawfile(output),charToRaw(expected[[i]])))
    unlink(output)
  }
  writeBin(charToRaw("ID,REC,SORT,KEEP\n"),data)
  result <- yamaa_domain(entry);actual <- result$issues
  stopifnot(nrow(actual)==1L,identical(actual$phase,"validation"),identical(actual$condition,"ordinal_field_collision"),
    identical(actual$requirement,"REQ-1294"),identical(actual$spec_paths,list("input.SRC.ordinal")),
    identical(actual$context,'{"dataset":"SRC","field":"REC"}'),is.null(result$output),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(output))
  cat("public raw source filter retains multiline record positions and internal presentation order with independent exact cached CSV twice without Python passed\n")
})

local({
  directory <- tempfile("public-string-padding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  cases <- file.path(root,"text-pad-cases")
  findings <- read.delim(file.path(root,"text-pad-findings.tsv"),quote="",colClasses="character",check.names=FALSE)
  for(name in list.dirs(cases,full.names=FALSE,recursive=FALSE)) {
    stopifnot(file.copy(file.path(cases,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    if(name=="width-noninteger") unlink(file.path(case,"input.csv"))
    result <- yamaa_domain(entry)
    expected_csv <- file.path(case,"expected.csv")
    if(file.exists(expected_csv)) {
      stopifnot(nrow(result$issues)==0L,!is.null(result$output),!file.exists(file.path(case,"output.csv")),
        identical(result$save(),TRUE),identical(rawfile(file.path(case,"output.csv")),rawfile(expected_csv)))
    } else {
      expected <- findings[findings$case==name,]
      actual <- result$issues
      stopifnot(nrow(actual)==nrow(expected),is.null(result$output),identical(actual$phase,expected$phase),
        identical(actual$condition,expected$condition),identical(actual$requirement,expected$requirement),
        identical(actual$context,expected$context))
      for(index in seq_len(nrow(expected))) {
        # Each captured padding cause has one independently authored full path.
        stopifnot(identical(paste0('["',actual$spec_paths[[index]],'"]'),expected$spec_paths[[index]]))
      }
      stopifnot(inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
      if(name=="width-noninteger") stopifnot(identical(yamaa_check(entry)$issues,actual),
        is.null(get(".domain_observations",asNamespace("yamaa"))(result)),!file.exists(file.path(case,"input.csv")))
    }
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    if(!is.null(observed)) {
      report <- rawToChar(observed)
      # Literal complete arrays avoid locale-dependent regexp offsets for UTF-8
      # report bytes on Windows and retain the independent empty-ledger truth.
      stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
        sum(gregexpr('"handler_counts":[],',report,fixed=TRUE)[[1L]]>0L)==2L)
    }
  }
  stopifnot(file.copy(file.path(root,"cases","negative-str-pad-width"),directory,recursive=TRUE))
  case <- file.path(directory,"negative-str-pad-width");unlink(file.path(case,"input"),recursive=TRUE)
  entry <- file.path(case,"spec.yaml");checked <- yamaa_check(entry)$issues;rejected <- yamaa_domain(entry)
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"invalid_field_type"),
    identical(checked$requirement,"REQ-0287"),identical(checked$spec_paths,list("columns.IDVARVAL.derivation.str_pad.width")),
    identical(checked$context,'{"actual":"str","expected":"int"}'),identical(rejected$issues,checked),
    is.null(rejected$output),!file.exists(file.path(case,"input")))
  cat("public canonical scalar padding matches complete independent CSV and findings with missing passthrough, no handlers and original width failure before input without Python passed\n")
})

local({
  source(file.path(root,"flag-truth.R"),local=TRUE)
  stopifnot(length(flag_truth)==33L)
  directory <- tempfile("flag-originals-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  observations <- get(".domain_observations",asNamespace("yamaa"))
  for(name in names(flag_truth)) {
    truth <- flag_truth[[name]]
    stopifnot(file.copy(file.path(root,truth$directory,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    stopifnot(identical(rawfile(entry),charToRaw(enc2utf8(truth$specification))))
    files <- list.files(case,recursive=TRUE,full.names=TRUE)
    retained <- lapply(files,rawfile)
    output <- file.path(case,truth$output)
    for(attempt in seq_len(2L)) {
      stopifnot(!file.exists(output))
      result <- yamaa_domain(entry)
      check_issue_frame(result$issues,truth$findings)
      wire <- observations(result)
      if(is.null(wire)) stopifnot(identical(truth$handlers,"[]")) else {
        report <- rawToChar(wire)
        ledger <- paste0('"handler_counts":',truth$handlers,',')
        stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
          sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
      }
      if(nzchar(truth$expected_csv)) {
        expected <- rawfile(file.path(case,truth$expected_csv))
        stopifnot(is.data.frame(result$output),isTRUE(result$save()),identical(rawfile(output),expected))
        unlink(output)
      } else stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
      stopifnot(!file.exists(output),identical(lapply(files,rawfile),retained))
      if(truth$schema_failure) stopifnot(is.null(wire))
    }
  }
  cat("declarative flags: six unchanged positives/six negatives and 21 authored complete captures match findings/CSV/handlers on two attempts without Python; schema provenance and two held report/case gates remain explicit\n")
})

local({
  source(file.path(root,"row-intermediate-truth.R"),local=TRUE)
  directory <- tempfile("public-row-intermediates-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(row_intermediate_truth)) {
    truth <- row_intermediate_truth[[name]]
    if(identical(name,"different-row-drivers")) stopifnot(truth$success,length(truth$issues$phase)==0L)
    stopifnot(file.copy(file.path(root,"row-cases",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    result <- yamaa_domain(entry);check_issue_frame(result$issues,truth$issues)
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    if(!is.null(observed)) {
      report <- rawToChar(observed)
      # Compare complete arrays literally; authored paths can contain brackets.
      ledger <- paste0('"handler_counts":',truth$ledger,',')
      stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
        sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
    }
    output <- file.path(case,truth$output)
    stopifnot(!file.exists(output))
    if(truth$success) {
      stopifnot(!is.null(result$output),identical(result$save(),TRUE),
        identical(rawfile(output),rawfile(file.path(case,"expected.csv"))),
        identical(result$save(),TRUE),identical(rawfile(output),rawfile(file.path(case,"expected.csv"))))
    } else {
      stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(output))
    }
    if(name %in% fixture_strings(host_truth[["row_driver_cases"]])) {
      check_issue_frame(yamaa_check(entry)$issues,truth$issues)
    }
  }
  original <- file.path(directory,"adam-adlb-supplb-padded-key")
  unlink(file.path(original,"input"),recursive=TRUE)
  stopifnot(nrow(yamaa_check(file.path(original,"spec.yaml"))$issues)==0L,!file.exists(file.path(original,"input")))
  cat("common input record rows match unchanged padded-key bytes, complete independent findings and reached handler ledgers without Python; EOT intermediate/grouped named-source execution passed\n")
})

local({
  directory <- tempfile("public-text-extract-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  handler_truth <- read.delim(file.path(root,"text-extract-handlers.tsv"),quote="",colClasses="character",check.names=FALSE)
  check_handlers <- function(result,case_name) {
    report <- get(".domain_observations",asNamespace("yamaa"))(result)
    expected <- handler_truth$handlers[handler_truth$case==case_name]
    if(!is.null(report) && length(expected)) stopifnot(length(expected)==1L,
      grepl(paste0('"handler_counts":',expected),rawToChar(report),fixed=TRUE))
  }
  case_name <- "adam-adae-text-cleanup"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
  stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
  result <- yamaa_domain(entry)
  check_handlers(result,case_name)
  expected <- read.csv(file.path(case,"expected","adae.csv"),colClasses="character",na.strings="",check.names=FALSE)
  for(column in c("AESEQ","AEREFNUM")) expected[[column]] <- structure(expected[[column]],class="yamaa_int64_vector")
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
    !file.exists(file.path(case,"adae.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adae.csv")),rawfile(file.path(case,"expected","adae.csv"))))
  case_name <- "negative-str-uncaptured-group"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues;rejected <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"regex_group_out_of_range"),
    identical(checked$requirement,"REQ-0816"),identical(checked$spec_paths,list("columns.SITEID.derivation.str_extract.group")),
    identical(checked$context,'{"group":2,"group_count":1,"pattern":"^CATH-([^-]+)-[0-9]{4}$"}'),
    identical(rejected$issues,checked),is.null(rejected$output),is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-extract-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,];case_name <- finding$case
    stopifnot(file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);rejected <- yamaa_domain(file.path(case,"spec.yaml"));actual <- rejected$issues
    check_handlers(rejected,case_name)
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),identical(actual$condition,finding$condition),
      identical(actual$requirement,finding$requirement),identical(actual$spec_paths,list(finding$spec_path)),identical(actual$context,finding$context),
      is.null(rejected$output),inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
  }
  for(case_name in c("optional-unentered","entered-empty","whole-empty","leftmost-alternative","last-repeated-capture","literal-null","nested-case","row-extract")) {
    stopifnot(file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);result <- yamaa_domain(file.path(case,"spec.yaml"))
    check_handlers(result,case_name)
    stopifnot(nrow(result$issues)==0L,!file.exists(file.path(case,"output.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv"))))
    if(case_name=="optional-unentered") stopifnot(identical(result$output$RESULT,NA_character_))
    if(case_name %in% c("entered-empty","whole-empty")) stopifnot(identical(result$output$RESULT,""))
  }
  cat("public str_extract matches unchanged ADAE bytes and complete independent failures/captures without Python passed\n")
})

local({
  directory <- tempfile("yamaa-template-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
  handlers <- read.delim(file.path(root,"text-template-handlers.tsv"),quote="",colClasses="character",check.names=FALSE)
  paths <- read.delim(file.path(root,"text-template-directories.tsv"),quote="",colClasses="character",check.names=FALSE)
  directories <- setNames(paths$directory,paths$case)
  check_handlers <- function(result,name) {
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    expected <- handlers$handlers[handlers$case==name]
    if(!is.null(observed) && length(expected)) stopifnot(length(expected)==1L,
      grepl(paste0('"handler_counts":',expected),rawToChar(observed),fixed=TRUE))
  }
  name <- "adam-adsl-site-parse"
  stopifnot(file.copy(file.path(root,"cases",name),directory,recursive=TRUE))
  case <- file.path(directory,name);entry <- file.path(case,"spec.yaml");written <- rawfile(entry)
  stopifnot(file.rename(file.path(case,"input"),file.path(case,"held")),nrow(yamaa_check(entry)$issues)==0L,
    file.rename(file.path(case,"held"),file.path(case,"input")))
  result <- yamaa_domain(entry);check_handlers(result,name)
  expected <- read.csv(file.path(case,"expected","adsl.csv"),colClasses="character",na.strings="",check.names=FALSE)
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
    !file.exists(file.path(case,"adsl.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adsl.csv")),rawfile(file.path(case,"expected","adsl.csv"))))
  name <- "negative-subject-reference"
  stopifnot(file.copy(file.path(root,"cases",name),directory,recursive=TRUE))
  case <- file.path(directory,name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues;result <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"invalid_string_template"),
    identical(checked$requirement,"REQ-0461"),identical(checked$spec_paths,list("columns.SUBJREF.derivation.str_template")),
    identical(checked$context,'{"placeholder":"SITEID + \':\' + SUBJID","reason":"invalid_placeholder"}'),
    identical(result$issues,checked),is.null(result$output),is.null(get(".domain_observations",asNamespace("yamaa"))(result)),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-template-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  failures$requirement[!nzchar(failures$requirement)] <- NA_character_
  for(name in unique(failures$case)) {
    stopifnot(file.copy(file.path(root,"text-template-cases",directories[[name]]),directory,recursive=TRUE))
    case <- file.path(directory,name)
    if(directories[[name]]!=name) stopifnot(file.rename(file.path(directory,directories[[name]]),case))
    result <- yamaa_domain(file.path(case,"spec.yaml"));actual <- result$issues
    expected <- failures[failures$case==name,];check_handlers(result,name)
    stopifnot(nrow(actual)==nrow(expected),identical(actual$phase,expected$phase),identical(actual$condition,expected$condition),
      identical(actual$requirement,expected$requirement),identical(actual$spec_paths,lapply(expected$spec_path,identity)),
      identical(actual$context,expected$context),is.null(result$output),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
  }
  cases <- sort(paths$case)
  for(name in cases) {
    if(name %in% failures$case || name=="nul-value") next
    # NUL scalar materialization is qualified at pure/native-codec and Python
    # boundaries; the existing R character facade does not admit embedded NUL.
    stopifnot(file.copy(file.path(root,"text-template-cases",directories[[name]]),directory,recursive=TRUE))
    case <- file.path(directory,name)
    if(directories[[name]]!=name) stopifnot(file.rename(file.path(directory,directories[[name]]),case))
    result <- yamaa_domain(file.path(case,"spec.yaml"));check_handlers(result,name)
    stopifnot(nrow(result$issues)==0L,!file.exists(file.path(case,"output.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv"))))
    if(name=="empty-template") stopifnot(identical(result$output$RESULT,""))
    if(name=="empty-string-value") stopifnot(identical(result$output$RESULT,"<>"))
    if(name=="missing-null") stopifnot(identical(result$output$RESULT,NA_character_))
  }
  cat("public str_template matches unchanged site bytes and complete ordered independent findings/handlers without Python passed\n")
})

local({
  source(file.path(root,"key-truth.R"),local=TRUE)
  stopifnot(length(key_truth)==19L)
  directory <- tempfile("public-intermediate-keys-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(key_truth)) {
    truth <- key_truth[[name]]
    stopifnot(file.copy(file.path(root,"keys",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);output <- file.path(case,"out.csv")
    for(attempt in seq_len(2L)) {
      stopifnot(!file.exists(output))
      result <- yamaa_domain(entry);check_issue_frame(result$issues,truth$issues)
      observed <- get(".domain_observations",asNamespace("yamaa"))(result)
      if(is.null(observed)) {
        stopifnot(identical(truth$handlers,"[]"))
      } else {
        report <- rawToChar(observed)
        ledger <- paste0('"handler_counts":',truth$handlers,',')
        stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
          sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
      }
      if(truth$success) {
        stopifnot(!is.null(result$output),identical(result$save(),TRUE),
          identical(rawfile(output),rawfile(file.path(case,"expected.csv"))))
        unlink(output)
      } else {
        stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
      }
      stopifnot(!file.exists(output),identical(rawfile(entry),written))
    }
  }
  cat("all 19 unchanged scalar-key captures match complete ordered findings, CSV and handler ledgers twice without Python passed\n")
})

local({
  source(file.path(root,"key-truth.R"),local=TRUE)
  original <- file.path(root,"keys","dependency_before_bad_filter")
  type_truth <- list(phase="validation",condition="incompatible_input_type",requirement="REQ-0190",
    spec_paths=list("intermediates[0].filter"),context='{"left_type":"int","right_type":"str"}')
  directory <- tempfile("retained-filter-types-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(variant in c("duplicate","resolved","empty_donor","empty_driver","missing_filter_operand","unused","unread_branch","boolean_missing")) {
    stopifnot(file.copy(original,directory,recursive=TRUE))
    case <- file.path(directory,"dependency_before_bad_filter")
    entry <- file.path(case,"spec.yaml");output <- file.path(case,"out.csv")
    written <- rawToChar(rawfile(entry))
    if(variant!="duplicate") writeBin(charToRaw("ID,N\na,7\n"),file.path(case,"src.csv"))
    if(variant=="empty_donor") writeBin(charToRaw("K,KINT,KNUM,VAL\n"),file.path(case,"tab.csv"))
    if(variant=="empty_driver") writeBin(charToRaw("ID,N\n"),file.path(case,"src.csv"))
    if(variant %in% c("missing_filter_operand","boolean_missing"))
      writeBin(charToRaw('K,KINT,KNUM,VAL\n"       7",,7,A\n'),file.path(case,"tab.csv"))
    if(variant=="unused") written <- gsub("derivation: LOOK.VAL","derivation: {literal: ok}",written,fixed=TRUE)
    if(variant=="unread_branch") written <- gsub("derivation: LOOK.VAL",
      "derivation: {case: [{when: 'FALSE', then: {source: LOOK.VAL}}, {otherwise: {literal: ok}}]}",written,fixed=TRUE)
    if(variant=="boolean_missing") written <- sub("TAB.KINT > 'bad'","FALSE AND TAB.KINT > 'bad'",written,fixed=TRUE)
    writeBin(charToRaw(written),entry)
    result <- yamaa_domain(entry)
    if(variant=="unused") {
      # Every specification is pruned after composition (REQ-0638, REQ-0641): no
      # column reads LOOK, so pruning discards the intermediate together with its
      # ill-typed filter and the specification builds with no issues instead of
      # refusing the dead declaration.
      stopifnot(nrow(result$issues)==0L,!is.null(result$output),identical(result$save(),TRUE),
        identical(rawfile(output),charToRaw("ID,V,V2\na,ok,ok\n")))
      unlink(file.path(case,c("src.csv","tab.csv")))
      stopifnot(nrow(yamaa_check(entry)$issues)==0L)
      unlink(case,recursive=TRUE)
      next
    }
    check_issue_frame(result$issues,if(variant=="duplicate") key_truth$dependency_before_bad_filter$issues else type_truth)
    stopifnot(is.null(result$output))
    for(i in seq_len(2L)) stopifnot(inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
    stopifnot(!file.exists(output))
    unlink(file.path(case,c("src.csv","tab.csv")))
    check_issue_frame(yamaa_check(entry)$issues,type_truth)
    unlink(case,recursive=TRUE)
  }
  cat("retained filter types, original key ordering and check without source files passed\n")
})

# Compare complete independently authored ledgers while ignoring only JSON object
# key order. Array order, every field, duplicate fields and integer text remain
# part of the comparison; no semantic observation is selected or discarded.
check_handler_ledger <- function(report, expected) {
  string <- '"(?:[^"\\\\]|\\\\.)*"'
  field <- paste0(string,':(?:',string,'|0|[1-9][0-9]*)')
  object <- paste0('\\{',field,'(?:,',field,')*\\}')
  array <- paste0('\\[(?:',object,'(?:,',object,')*)?\\]')
  canonical <- function(value) {
    stopifnot(grepl(paste0('^',array,'$'),value,perl=TRUE))
    objects <- regmatches(value,gregexpr(object,value,perl=TRUE))[[1L]]
    ordered <- vapply(objects,function(value) {
      fields <- regmatches(value,gregexpr(field,value,perl=TRUE))[[1L]]
      paste0('{',paste(sort(fields,method="radix"),collapse=','),'}')
    },character(1L),USE.NAMES=FALSE)
    paste0('[',paste(ordered,collapse=','),']')
  }
  prefix <- '"handler_counts":'
  ledgers <- regmatches(report,gregexpr(paste0(prefix,array),report,perl=TRUE))[[1L]]
  stopifnot(sum(gregexpr(prefix,report,fixed=TRUE)[[1L]]>0L)==2L,
    length(ledgers)==2L,
    identical(vapply(substring(ledgers,nchar(prefix)+1L),canonical,character(1L),USE.NAMES=FALSE),
      rep(canonical(expected),2L)))
}
local({
  expected <- '[{"spec_path":"rows[1].V","handler":"multiple_matches","count":2}]'
  observed <- '[{"count":2,"handler":"multiple_matches","spec_path":"rows[1].V"}]'
  report <- function(value) paste0('{"handler_counts":',value,',"node":{"handler_counts":',value,'}}')
  check_handler_ledger(report(observed),expected)
  for(value in c(sub('"count":2','"count":1',observed,fixed=TRUE),
    sub(',"handler":"multiple_matches"','',observed,fixed=TRUE),
    sub('"count":2','"count":2,"extra":0',observed,fixed=TRUE),
    sub('"count":2','"count":2,"count":2',observed,fixed=TRUE),
    paste0('[',substring(observed,2L,nchar(observed)-1L),',',substring(observed,2L,nchar(observed)-1L),']'))) {
    stopifnot(inherits(tryCatch(check_handler_ledger(report(value),expected),error=identity),"error"))
  }
  ordered <- '[{"count":1,"handler":"no_match","spec_path":"A"},{"count":2,"handler":"no_match","spec_path":"B"}]'
  reversed <- '[{"count":2,"handler":"no_match","spec_path":"B"},{"count":1,"handler":"no_match","spec_path":"A"}]'
  stopifnot(inherits(tryCatch(check_handler_ledger(report(reversed),ordered),error=identity),"error"))
})

local({
  body <- host_truth[["section_order_yaml"]]
  directory <- tempfile("yamaa-self-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n"),file.path(directory,"src.csv"))
  writeBin(charToRaw("ID,K\nc,x\nd,x\n"),file.path(directory,"other.csv"))
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),identical(rawfile(file.path(directory,"result.csv")),fixture_bytes(host_truth[["section_order_csv"]])))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,"[{\"spec_path\":\"rows[1].derivations.V.compute.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows[2].derivations.V.compute.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":2},{\"spec_path\":\"columns.FINAL.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5},{\"spec_path\":\"columns.FIRST.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5}]")

  unlink(file.path(directory,fixture_strings(host_truth[["section_case_files"]])))
  writeBin(charToRaw(sub("SECTION: {literal: first}","SECTION: {source: {variable: P.K}}",body,fixed=TRUE)),entry)
  expected <- list(phase="validation",condition="phase_boundary",requirement="REQ-0120",spec_paths=list("rows[0].derivations.SECTION.source"),context="{\"available_phase\":\"completed_prior_row_template\",\"identifier\":\"P.K\",\"required_phase\":\"row_construction\"}")
  check_issue_frame(yamaa_check(entry)$issues,expected)
  rejected <- yamaa_domain(entry);check_issue_frame(rejected$issues,expected)
  stopifnot(is.null(rejected$output),is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"src.csv")),!file.exists(file.path(directory,"result.csv")))
  cat("completed-template SELF borrowing, complete handlers and pre-study phase refusal passed without Python\n")
})

local({
  body <- host_truth[["duplicate_lookup_yaml"]]
  directory <- tempfile("yamaa-intermediate-admission-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID\n2\n1\n3\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["duplicate_donor_csv"]]),file.path(directory,"donor.csv"))
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),
    identical(rawfile(file.path(directory,"result.csv")),fixture_bytes(host_truth[["duplicate_lookup_csv"]])))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,'[{"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},{"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},{"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},{"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}]')
  unlink(c(file.path(directory,"src.csv"),file.path(directory,"donor.csv"),file.path(directory,"result.csv")))
  cases <- list(
    list(remove=host_truth[["lookup_first_clause"]],condition="rename_only_intermediate",requirement="REQ-1248",context='{"dataset":"DONOR","intermediate":"P"}'),
    list(remove="    keep: first\n",condition="unpaired_fields",requirement="REQ-0119",context='{"declared":["order_by"],"intermediate":"P","missing":["keep"]}'),
    list(remove="    order_by: [DONOR.SEQ]\n",condition="unpaired_fields",requirement="REQ-0119",context='{"declared":["keep"],"intermediate":"P","missing":["order_by"]}'))
  for(case in cases) {
    writeBin(charToRaw(sub(case$remove,"",body,fixed=TRUE)),entry)
    expected <- list(phase="validation",condition=case$condition,requirement=case$requirement,
      spec_paths=list("intermediates[0]"),context=case$context)
    checked <- yamaa_check(entry);check_issue_frame(checked$issues,expected)
    rejected <- yamaa_domain(entry);check_issue_frame(rejected$issues,expected)
    stopifnot(is.null(rejected$output),is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
      inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"src.csv")))
  }
  cat("inferred applicable intermediate keys, stable shared selection and complete declaration failures passed without Python\n")
})

local({
  body <- host_truth[["lookup_filter_yaml"]]
  directory <- tempfile("yamaa-primary-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(fixture_bytes(host_truth[["filtered_donor_csv"]]),file.path(directory,"src.csv"))
  stopifnot(nrow(yamaa_check(entry)$issues)==0L)
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),
    identical(rawfile(file.path(directory,"result.csv")),fixture_bytes(host_truth[["filtered_lookup_csv"]])))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,'[{"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":2},{"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},{"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":2},{"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}]')
  cat("same-input donor grain, shared missing field and complete handler accounting passed without Python\n")
})

local({
  directory <- tempfile("yamaa-correlated-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw("ID,SEQ,CUT\n1,1,2\n1,2,4\n3,1,2\n1,3,\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["ordered_donor_csv"]]),file.path(directory,"donor.csv"))
  writeBin(charToRaw("ID,SEQ,CUT\n1,1,4\n3,1,2\n"),file.path(directory,"alt.csv"))
  writeBin(fixture_bytes(host_truth[["lookup_cutoff_yaml"]]),entry)
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),identical(rawfile(file.path(directory,"result.csv")),fixture_bytes(host_truth[["lookup_cutoff_csv"]])))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,"[{\"spec_path\":\"columns.A.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.A.derivation.source.no_match\",\"handler\":\"no_match\",\"count\":2},{\"spec_path\":\"columns.B.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.B.derivation.source.no_match\",\"handler\":\"no_match\",\"count\":2}]")
  unlink(file.path(directory,"result.csv"))
  writeBin(fixture_bytes(host_truth[["lookup_alternative_yaml"]]),entry)
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),identical(rawfile(file.path(directory,"result.csv")),charToRaw("ID,SEQ,A,B\n1,1,three,\n3,1,,\n")))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,"[{\"spec_path\":\"rows[0].derivations.A.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows[0].derivations.B.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows[0].derivations.A.source.no_match\",\"handler\":\"no_match\",\"count\":1},{\"spec_path\":\"rows[0].derivations.B.source.no_match\",\"handler\":\"no_match\",\"count\":1}]")
  unlink(file.path(directory,"result.csv"))
  writeBin(charToRaw("schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID, SEQ]\ninput:\n  SRC: {path: src.csv, types: {ID: int, SEQ: int, CUT: int}}\n  ALT: {path: alt.csv, types: {ID: int, SEQ: int, CUT: int}}\n  D: {path: donor.csv, types: {ID: int, TIME: int}}\noutput: {path: result.csv, columns: [ID, SEQ, A, B]}\nintermediates:\n  - id: P\n    dataset: D\n    key: [ID]\n    filter: \"D.TIME < SRC.CUT\"\n    order_by: [D.TIME]\n    keep: last\n    no_match: null\nrows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, SEQ: ALT.SEQ, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: SEQ, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}\n"),entry)
  rejected <- yamaa_domain(entry)
  check_issue_frame(rejected$issues,list(phase="validation",condition="unknown_field",requirement="REQ-0120",spec_paths=list("intermediates[0].filter"),context='{"identifier":"SRC.CUT","intermediate":"P"}'))
  stopifnot(is.null(rejected$output),inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"result.csv")))
  cat("correlated donor filters, actual current driver, complete findings and handler accounting passed without Python\n")
})

local({
  body <- host_truth[["lookup_fallback_yaml"]]
  directory <- tempfile("yamaa-case-intermediates-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID,SELECT\n1,Y\n2,N\n3,Y\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["fallback_donor_csv"]]),file.path(directory,"donor.csv"))
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),
    identical(rawfile(file.path(directory,"result.csv")),charToRaw("ID,A,B\n1,first,\n2,skip,skip\n3,,\n")))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,"[{\"spec_path\":\"columns.A.derivation.case[0].then.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.A.derivation.case[0].then.source.no_match\",\"handler\":\"no_match\",\"count\":1},{\"spec_path\":\"columns.B.derivation.case[0].then.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.B.derivation.case[0].then.source.no_match\",\"handler\":\"no_match\",\"count\":1}]")
  for (variable in c("P.A", "P.ABSENT")) {
    writeBin(charToRaw(sub("then: P.A",paste0("then: {source: {variable: ",variable,", absent: fallback}}"),body,fixed=TRUE)),entry)
    admitted <- yamaa_domain(entry)
    expected <- if (variable == "P.A") "ID,A,B\n1,first,\n2,skip,skip\n3,,\n" else host_truth[["fallback_lookup_csv"]]
    count <- if (variable == "P.A") "0" else "2"
    stopifnot(nrow(admitted$issues)==0L, admitted$save(),
      identical(rawfile(file.path(directory,"result.csv")), charToRaw(expected)))
    observations <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(admitted))
    handler <- paste0('{"count":',count,',"handler":"absent","spec_path":"columns.A.derivation.case[0].then.source.absent"}')
    stopifnot(grepl(handler,observations,fixed=TRUE))
  }
  cat("nested case intermediate source shares lazy kept selection and complete handler accounting without Python\n")
})


local({
  failures <- read.delim(file.path(root,"case-predicate-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  for (case in list.dirs(file.path(root,"case-predicate-cases"),full.names=TRUE,recursive=FALSE)) {
    directory <- tempfile("yamaa-case-predicate-");dir.create(directory)
    file.copy(list.files(case,full.names=TRUE),directory)
    entry <- file.path(directory,"spec.yaml")
    result <- yamaa_domain(entry)
    expected <- file.path(directory,"expected.csv")
    if (file.exists(expected)) {
      stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),identical(rawfile(file.path(directory,"result.csv")),rawfile(expected)))
    } else {
      finding <- failures[failures$case==basename(case),,drop=FALSE]
      stopifnot(nrow(finding)==1L)
      check_issue_frame(result$issues,list(phase=finding$phase,condition=finding$condition,requirement=finding$requirement,spec_paths=list(finding$spec_path),context=finding$context))
      stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"result.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  for (name in fixture_strings(host_truth[["source_selection_case_names"]])) {
    case <- file.path(root,"cases",name)
    directory <- tempfile("yamaa-original-case-");dir.create(directory)
    file.copy(list.files(case,full.names=TRUE),directory,recursive=TRUE)
    result <- yamaa_domain(file.path(directory,"spec.yaml"))
    stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE))
    for (expected in list.files(file.path(directory,"expected"),pattern="\\.csv$",full.names=TRUE)) stopifnot(identical(rawfile(file.path(directory,basename(expected))),rawfile(expected)))
    unlink(directory,recursive=TRUE)
  }
  cat("case predicates, branch filters and unchanged original CSV truth passed without Python\n")
})

local({
  # Historical private truth is retained separately from the normative correction.
  source(file.path(root,"case-predicate-dispositions.R"),local=TRUE)
  for (name in names(case_predicate_dispositions)) {
    disposition <- case_predicate_dispositions[[name]]
    stopifnot(identical(disposition$classification,"historical_private_reference_defect"),
      identical(disposition$requirement,"REQ-0148"),
      identical(rawfile(file.path(root,"case-predicate-history",name,"expected.csv")),
        charToRaw(disposition$historical_csv)))
  }
  for (label in fixture_strings(host_truth[["source_selection_modes"]])) {
  source(file.path(root,paste0(label,"-truth.R")),local=TRUE)
  for (name in names(source_selection_truth)) for (attempt in seq_len(2L)) {
    truth <- source_selection_truth[[name]]
    directory <- tempfile("yamaa-source-selection-");dir.create(directory)
    file.copy(list.files(file.path(root,paste0(label,"-cases"),truth$directory),full.names=TRUE),directory)
    entry <- file.path(directory,"spec.yaml")
    check_issue_frame(yamaa_check(entry)$issues,truth$check_issues)
    result <- yamaa_domain(entry)
    check_issue_frame(result$issues,truth$issues)
    report <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
    check_handler_ledger(report,truth$handlers)
    expected_reads <- paste0('"source_reads":',truth$source_reads)
    stopifnot(grepl(expected_reads,report,fixed=TRUE),grepl('"artifacts":[]',report,fixed=TRUE))
    if (!is.null(truth$columns)) {
      stopifnot(identical(names(result$output),names(truth$columns)))
      for (column in names(truth$columns)) {
        actual <- result$output[[column]]
        if (truth$kinds[[column]] == "int") stopifnot(inherits(actual,"yamaa_int64_vector"))
        else stopifnot(is.character(actual))
        stopifnot(identical(is.na(actual),is.na(truth$columns[[column]])),
          identical(as.character(actual),truth$columns[[column]]))
      }
      stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv"))))
      writeBin(charToRaw("changed after build"),file.path(directory,"input.csv"))
      stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv"))))
    } else stopifnot(is.null(result$output),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"result.csv")))
    unlink(directory,recursive=TRUE)
  }
  }
  cat("independent source selection, key-phase ordering, full int64 cells and retained bytes passed without Python\n")
})

local({
  label <- "completed-lookup-assertions"
  source(file.path(root,paste0(label,"-truth.R")),local=TRUE)
  stopifnot(length(completed_lookup_assertion_truth)==21L)
  for (name in names(completed_lookup_assertion_truth)) for (attempt in seq_len(2L)) {
    truth <- completed_lookup_assertion_truth[[name]]
    directory <- tempfile("yamaa-completed-assert-");dir.create(directory)
    directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
    file.copy(list.files(file.path(root,paste0(label,"-cases"),truth$directory),full.names=TRUE),directory)
    entry <- file.path(directory,"spec.yaml")
    check_issue_frame(yamaa_check(entry)$issues,truth$check_issues)
    result <- yamaa_domain(entry)
    check_issue_frame(result$issues,truth$issues)
    report <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
    check_handler_ledger(report,truth$handlers)
    if (!is.null(truth$verifications)) {
      expected <- gsub('"__SPECIFICATION__"',json_text(entry),truth$verifications,fixed=TRUE)
      stopifnot(grepl(paste0('"verifications":',expected),report,fixed=TRUE))
    }
    expected_reads <- paste0('"source_reads":',truth$source_reads)
    stopifnot(grepl(expected_reads,report,fixed=TRUE),grepl('"artifacts":[]',report,fixed=TRUE))
    if (!is.null(truth$columns)) {
      stopifnot(identical(names(result$output),names(truth$columns)))
      for (column in names(truth$columns)) {
        actual <- result$output[[column]]
        if (truth$kinds[[column]] == "int") stopifnot(inherits(actual,"yamaa_int64_vector"))
        else stopifnot(is.character(actual))
        stopifnot(identical(is.na(actual),is.na(truth$columns[[column]])),
          identical(as.character(actual),truth$columns[[column]]))
      }
      stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv"))))
      writeBin(charToRaw("changed after build"),file.path(directory,"donor.csv"))
      stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv"))))
    } else stopifnot(is.null(result$output),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"result.csv")))
    unlink(directory,recursive=TRUE)
  }
  cat("independent completed-row selected assertions retain exact records, effects and Save bytes without Python\n")
})

local({
  for (truth in host_truth[["inheritance_nulls"]]) {
    directory <- tempfile("yamaa-inheritance-null-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$spec),entry)
    for (name in names(truth$parents)) writeBin(charToRaw(truth$parents[[name]]),file.path(directory,name))
    checked <- yamaa_check(entry)$issues
    if (is.null(truth$check_condition)) stopifnot(nrow(checked)==0L)
    else stopifnot(identical(checked$condition[[1]],truth$check_condition))
    for (name in names(truth$inputs)) writeBin(charToRaw(truth$inputs[[name]]),file.path(directory,name))
    result <- yamaa_domain(entry)
    if (is.null(truth$run_condition)) {
      stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),charToRaw(truth$csv)))
    } else stopifnot(identical(result$issues$condition[[1]],truth$run_condition),is.null(result$output))
    observations <- get(".domain_observations",asNamespace("yamaa"))(result)
    if (!is.null(observations)) {
      counts <- jsonlite::fromJSON(rawToChar(observations),simplifyVector=FALSE)$handler_counts
      reached <- sum(vapply(counts,function(item) if(item$handler=="no_match") item$count else 0,numeric(1)))
      stopifnot(reached==truth$no_match_count)
    }
    unlink(directory,recursive=TRUE)
  }
  cat("introducing missing handlers and inherited clears agree across public host boundaries\n")
})

local({
  for (truth in host_truth[["typed_intermediates"]]) {
    directory <- tempfile("yamaa-typed-intermediate-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$spec),entry)
    # Static checks must not access the CSV files, which do not exist yet.
    checked <- yamaa_check(entry)$issues
    if (is.null(truth$check_condition)) stopifnot(nrow(checked)==0L)
    else stopifnot(truth$check_condition %in% checked$condition)
    for (name in names(truth$inputs)) writeBin(charToRaw(truth$inputs[[name]]),file.path(directory,name))
    result <- yamaa_domain(entry)
    if (is.null(truth$run_condition)) {
      stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"result.csv")),charToRaw(truth$csv)))
    } else stopifnot(truth$run_condition %in% result$issues$condition,is.null(result$output))
    observations <- get(".domain_observations",asNamespace("yamaa"))(result)
    if (!is.null(observations)) {
      counts <- jsonlite::fromJSON(rawToChar(observations),simplifyVector=FALSE)$handler_counts
      reached <- sum(vapply(counts,function(item) if(item$handler=="unconvertible") item$count else 0,numeric(1)))
      stopifnot(reached==truth$unconvertible_count)
    }
    unlink(directory,recursive=TRUE)
  }
  cat("typed intermediate results convert before numeric matching and retain conversion handlers without Python\n")
})

local({
  assert_temporal_issues <- function(actual, expected) {
    if (is.null(expected)) stopifnot(nrow(actual)==0L)
    else stopifnot(any(vapply(seq_len(nrow(actual)),function(i)
      actual$phase[i]==expected$phase && actual$condition[i]==expected$condition &&
      actual$requirement[i]==expected$requirement &&
      identical(actual$spec_paths[[i]],expected$spec_path),logical(1))))
  }
  for (truth in host_truth[["temporal_arithmetic"]]) {
    directory <- tempfile("yamaa-temporal-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$spec),entry)
    assert_temporal_issues(yamaa_check(entry)$issues,truth$check_issue)
    for (name in names(truth$inputs)) writeBin(charToRaw(truth$inputs[[name]]),file.path(directory,name))
    result <- yamaa_domain(entry)
    assert_temporal_issues(result$issues,truth$run_issue)
    if (!is.null(truth$csv)) stopifnot(identical(result$save(),TRUE),
      identical(rawfile(file.path(directory,"result.csv")),charToRaw(truth$csv)))
    else {
      stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"result.csv")))
    }
    observations <- get(".domain_observations",asNamespace("yamaa"))(result)
    if (!is.null(observations)) {
      counts <- jsonlite::fromJSON(rawToChar(observations),simplifyVector=FALSE)$handler_counts
      reached <- sum(vapply(counts,function(item) if(item$handler=="invalid") item$count else 0,numeric(1)))
      stopifnot(reached==truth$invalid_count)
    }
    unlink(directory,recursive=TRUE)
  }
  cat("shared temporal arithmetic conformance preserves civil values and duration handlers without Python\n")
})

local({
  for (truth in host_truth[["namespace_admission"]]) {
    directory <- tempfile("namespace-admission-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    original <- rawfile(entry)
    # No input file exists: declaration admission must precede source access.
    for (attempt in 1:2) {
      checked <- yamaa_check(entry)
      result <- yamaa_domain(entry)
      for (frame in list(checked$issues,result$issues)) {
        stopifnot(nrow(frame)==length(truth$issues))
        for (index in seq_along(truth$issues)) {
          expected <- truth$issues[[index]]
          stopifnot(identical(frame$phase[[index]],expected$phase),
            identical(frame$condition[[index]],expected$condition),
            identical(frame$requirement[[index]],expected$requirement),
            identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
            context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
        }
      }
      stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")),identical(rawfile(entry),original))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("original namespace declarations preserve complete paired public refusals before source reads\n")
})

local({
  for (truth in host_truth[["predicate_suggestions"]]) {
    directory <- tempfile("predicate-suggestions-"); dir.create(directory)
    for (name in names(truth$inputs)) {
      source <- file.path(directory,name)
      dir.create(dirname(source),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),source)
    }
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    for (attempt in 1:2) {
      checked <- yamaa_check(entry); result <- yamaa_domain(entry)
      for (check in list(list(frame=checked$issues,expected=truth$check_issues),
                         list(frame=result$issues,expected=truth$issues))) {
        frame <- check$frame
        stopifnot(nrow(frame)==length(check$expected))
        for (index in seq_along(check$expected)) {
          expected <- check$expected[[index]]
          stopifnot(identical(frame$phase[[index]],expected$phase),
            identical(frame$condition[[index]],expected$condition),
            identical(frame$requirement[[index]],expected$requirement),
            identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
            context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
        }
      }
      if (!is.null(truth$csv)) stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
      else stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("original predicate qualified suggestions preserve unknown names and paired output\n")
})

local({
  for (truth in host_truth[["intermediate_dependencies"]]) {
    directory <- tempfile("intermediate-dependencies-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    for (inputs_present in c(FALSE,TRUE)) {
      if (inputs_present) for (name in names(truth$inputs)) {
        path <- file.path(directory,name)
        dir.create(dirname(path),recursive=TRUE,showWarnings=FALSE)
        writeBin(charToRaw(truth$inputs[[name]]),path)
      } else if (!length(truth$issues)) next
      for (attempt in 1:2) {
        checked <- yamaa_check(entry)
        result <- yamaa_domain(entry)
        for (frame in list(checked$issues,result$issues)) {
          stopifnot(nrow(frame)==length(truth$issues))
          for (index in seq_along(truth$issues)) {
            expected <- truth$issues[[index]]
            stopifnot(identical(frame$phase[[index]],expected$phase),
              identical(frame$condition[[index]],expected$condition),
              identical(frame$requirement[[index]],expected$requirement),
              identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
              context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
          }
        }
        stopifnot(!file.exists(file.path(directory,"out.csv")))
        if (length(truth$issues)) stopifnot(is.null(result$output),
          is.null(get(".domain_observations",asNamespace("yamaa"))(result)),
          inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
        else {
          if (!is.null(truth$output_rows)) stopifnot(identical(result$output$K,"r1"),
            identical(result$output$XVAL,1))
          stopifnot(identical(result$save(),TRUE),
            identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
          unlink(file.path(directory,"out.csv"))
        }
      }
    }
    stopifnot(identical(rawfile(entry),charToRaw(truth$specification)))
    unlink(directory,recursive=TRUE)
  }
  cat("original intermediate cycles and forward donor dependencies preserve complete paired findings and saved bytes\n")
})

local({
  for (truth in host_truth[["intermediate_row_drivers"]]) {
    directory <- tempfile("intermediate-row-drivers-"); dir.create(directory)
    for (name in names(truth$inputs)) {
      source <- file.path(directory,name)
      dir.create(dirname(source),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),source)
    }
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    for (attempt in 1:2) {
      checked <- yamaa_check(entry); result <- yamaa_domain(entry)
      for (check in list(list(frame=checked$issues,expected=truth$check_issues),
                         list(frame=result$issues,expected=truth$issues))) {
        frame <- check$frame
        stopifnot(nrow(frame)==length(check$expected))
        for (index in seq_along(check$expected)) {
          expected <- check$expected[[index]]
          stopifnot(identical(frame$phase[[index]],expected$phase),
            identical(frame$condition[[index]],expected$condition),
            identical(frame$requirement[[index]],expected$requirement),
            identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
            context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
        }
      }
      if (!is.null(truth$csv)) stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
      else stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("source-only named row drivers preserve record order and governed lookup keys\n")
})

local({
  assert_diagnostic_sites <- function(actual,expected) {
    stopifnot(identical(names(actual),fixture_strings(host_truth[["issue_columns"]])),
      nrow(actual)==length(expected))
    for (i in seq_along(expected)) {
      finding <- expected[[i]]
      context <- jsonlite::fromJSON(actual$context[[i]],simplifyVector=FALSE)
      stopifnot(identical(actual$phase[[i]],finding$phase),
        identical(actual$condition[[i]],finding$condition),
        identical(actual$requirement[[i]],finding$requirement),
        identical(actual$spec_paths[[i]],fixture_strings(finding$spec_paths)),
        context_equal(context,finding$context))
    }
  }
  for (truth in host_truth[["diagnostic_sites"]]) {
    directory <- tempfile("yamaa-diagnostic-sites-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    assert_diagnostic_sites(yamaa_check(entry)$issues,truth$check_issues)
    for (name in names(truth$inputs)) {
      path <- file.path(directory,name); dir.create(dirname(path),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),path)
    }
    for (attempt in 1:2) {
      assert_diagnostic_sites(yamaa_check(entry)$issues,truth$check_issues)
      result <- yamaa_domain(entry)
      assert_diagnostic_sites(result$issues,truth$issues)
      stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,truth$output)))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("original unknown references retain authored identifiers, dataset context and complete findings without Python\n")
})

local({
  for (truth in host_truth[["row_count_lookups"]]) {
    directory <- tempfile("row-count-lookups-"); dir.create(directory)
    directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
    for (name in names(truth$inputs)) {
      source <- file.path(directory,name)
      dir.create(dirname(source),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),source)
    }
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    for (attempt in 1:2) {
      checked <- yamaa_check(entry); result <- yamaa_domain(entry)
      for (check in list(list(frame=checked$issues,expected=truth$check_issues),
                         list(frame=result$issues,expected=truth$issues))) {
        frame <- check$frame
        stopifnot(nrow(frame)==length(check$expected))
        for (index in seq_along(check$expected)) {
          expected <- check$expected[[index]]
          stopifnot(identical(frame$phase[[index]],expected$phase),
            identical(frame$condition[[index]],expected$condition),
            identical(frame$requirement[[index]],expected$requirement),
            identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
            context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
        }
      }
      report <- jsonlite::fromJSON(rawToChar(get(".domain_observations",asNamespace("yamaa"))(result)),simplifyVector=FALSE)
      expected <- lapply(truth$verifications,function(record) {
        record$specification <- entry
        record
      })
      stopifnot(identical(report$verifications,expected),
        identical(report$handler_counts,truth$handler_counts))
      if (!is.null(truth$csv)) stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
      else stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("original row-count lookup predicates preserve typed scopes and paired output\n")
})

local({
  for (truth in host_truth[["baseline_window_types"]]) {
    directory <- tempfile("baseline-window-types-"); dir.create(directory)
    for (name in names(truth$inputs)) {
      source <- file.path(directory,name)
      dir.create(dirname(source),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),source)
    }
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    for (attempt in 1:2) {
      checked <- yamaa_check(entry); result <- yamaa_domain(entry)
      for (check in list(list(frame=checked$issues,expected=truth$check_issues),
                         list(frame=result$issues,expected=truth$issues))) {
        frame <- check$frame
        stopifnot(nrow(frame)==length(check$expected))
        for (index in seq_along(check$expected)) {
          expected <- check$expected[[index]]
          stopifnot(identical(frame$phase[[index]],expected$phase),
            identical(frame$condition[[index]],expected$condition),
            identical(frame$requirement[[index]],expected$requirement),
            identical(frame$spec_paths[[index]],fixture_strings(expected$spec_paths)),
            context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),expected$context))
        }
      }
      if (!is.null(truth$csv)) stopifnot(identical(result$save(),TRUE),
        identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
      else stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("baseline temporal operands preserve typed refusals and valid carriers\n")
})

local({
  assert_row_join_issues <- function(actual,expected) {
    stopifnot(identical(names(actual),fixture_strings(host_truth[["issue_columns"]])),
      nrow(actual)==length(expected))
    for (i in seq_along(expected)) {
      finding <- expected[[i]]
      context <- jsonlite::fromJSON(actual$context[[i]],simplifyVector=FALSE)
      stopifnot(identical(actual$phase[[i]],finding$phase),
        identical(actual$condition[[i]],finding$condition),
        identical(actual$requirement[[i]],finding$requirement),
        identical(actual$spec_paths[[i]],fixture_strings(finding$spec_paths)),
        context_equal(context,finding$context))
    }
  }
  stopifnot(length(host_truth[["row_join_keys"]])==11L)
  for (truth in host_truth[["row_join_keys"]]) {
    directory <- tempfile("yamaa-row-join-keys-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    assert_row_join_issues(yamaa_check(entry)$issues,truth$check_issues)
    for (name in names(truth$inputs)) {
      path <- file.path(directory,name); dir.create(dirname(path),recursive=TRUE,showWarnings=FALSE)
      writeBin(charToRaw(truth$inputs[[name]]),path)
    }
    for (attempt in 1:2) {
      assert_row_join_issues(yamaa_check(entry)$issues,truth$check_issues)
      result <- yamaa_domain(entry)
      assert_row_join_issues(result$issues,truth$issues)
      if (!is.null(truth$csv)) {
        stopifnot(!is.null(result$output),identical(result$save(),TRUE),
          identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
        unlink(file.path(directory,"out.csv"))
      } else stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("implicit row joins preserve raw driver keys while column and named lookups preserve constructed keys without Python\n")
})

local({
  assert_key_issues <- function(frame, expected) {
    stopifnot(nrow(frame)==length(expected))
    for (index in seq_along(expected)) {
      row <- expected[[index]]
      requirement <- if(is.null(row$requirement)) NA_character_ else row$requirement
      stopifnot(identical(frame$phase[[index]],row$phase),
        identical(frame$condition[[index]],row$condition),
        identical(frame$requirement[[index]],requirement),
        identical(frame$spec_paths[[index]],fixture_strings(row$spec_paths)),
        context_equal(jsonlite::fromJSON(frame$context[[index]],simplifyVector=FALSE),row$context))
    }
  }
  for (truth in host_truth[["key_expressions"]]) {
    directory <- tempfile("key-expressions-"); dir.create(directory)
    entry <- file.path(directory,"spec.yaml")
    writeBin(charToRaw(truth$specification),entry)
    if (length(truth$check_issues)) {
      assert_key_issues(yamaa_check(entry)$issues,truth$check_issues)
      assert_key_issues(yamaa_domain(entry)$issues,truth$check_issues)
    }
    for (name in names(truth$inputs)) writeBin(charToRaw(truth$inputs[[name]]),file.path(directory,name))
    for (attempt in 1:2) {
      assert_key_issues(yamaa_check(entry)$issues,truth$check_issues)
      result <- yamaa_domain(entry)
      assert_key_issues(result$issues,truth$issues)
      stopifnot(!file.exists(file.path(directory,"out.csv")))
      if (length(truth$issues)) stopifnot(is.null(result$output),
        inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),
        !file.exists(file.path(directory,"out.csv")))
      else {
        observations <- get(".domain_observations",asNamespace("yamaa"))(result)
        counts <- jsonlite::fromJSON(rawToChar(observations),simplifyVector=FALSE)$handler_counts
        stopifnot(identical(counts,truth$handlers),identical(result$save(),TRUE),
          identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
        writeBin(charToRaw("changed after build"),file.path(directory,"source.csv"))
        stopifnot(identical(result$save(),TRUE),
          identical(rawfile(file.path(directory,"out.csv")),charToRaw(truth$csv)))
        unlink(file.path(directory,"out.csv"))
        writeBin(charToRaw(truth$inputs[["source.csv"]]),file.path(directory,"source.csv"))
      }
    }
    stopifnot(identical(rawfile(entry),charToRaw(truth$specification)))
    unlink(directory,recursive=TRUE)
  }
  cat("scalar and window key expressions preserve current scope, raw comparisons, handler counts and saved truth without Python\n")
})

stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
