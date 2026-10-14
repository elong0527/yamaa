# Real public filesystem controls; private cache/limits/Changed stay in typed Rust.
library(yamaa)
root <- system.file("specification-original", package="yamaa", mustWork=TRUE)
source(file.path(root,"public-output-truth.R"),local=TRUE)
observations <- get(".domain_observations",envir=asNamespace("yamaa"))
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("public-files-native-only-")
dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))

json_text <- function(value) {
  # Quote only run-identity labels; no semantic field is normalized.
  characters <- strsplit(enc2utf8(value),"",fixed=TRUE)[[1L]]
  escaped <- vapply(characters,function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code),"8"="\\b","9"="\\t","10"="\\n","12"="\\f","13"="\\r",
      "34"="\\\"","92"="\\\\",if(code<32L) sprintf("\\u%04x",code) else character)
  },character(1L),USE.NAMES=FALSE)
  paste0('"',paste0(escaped,collapse=""),'"')
}
expected_report <- function(text,entry,artifacts=FALSE) {
  text <- sub('"runtime":"python"','"runtime":"r"',text,fixed=TRUE)
  text <- sub("fixture-runtime",as.character(getRversion()),text,fixed=TRUE)
  text <- sub("fixture-engine",yamaa:::.core_version(),text,fixed=TRUE)
  text <- sub('"example":"schema-lookup"','"example":"domain"',text,fixed=TRUE)
  text <- gsub('"specification":"spec.yaml"',
    paste0('"specification":',json_text(entry)),text,fixed=TRUE)
  if(!artifacts) text <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',text)
  text
}
lookup_report <- function(entry,artifacts=FALSE) {
  expected_report(rawToChar(rawfile(file.path(root,"expected/schema-lookup.json"))),entry,artifacts)
}
new_case <- function() {
  directory <- tempfile("public-file-owner-");dir.create(directory)
  stopifnot(file.copy(file.path(root,"cases/schema-lookup"),directory,recursive=TRUE))
  case <- file.path(directory,"schema-lookup")
  list(directory=directory,case=case,entry=normalizePath(file.path(case,"spec.yaml"),winslash="/"))
}
paths <- c(DM="input/dm.csv",AE="input/ae.csv",MEDDRA="input/meddict.csv")
inspection <- read.delim(file.path(root,"source-inspection.tsv"),sep="\t",quote="",
  comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
capture <- read.delim(file.path(root,"source-capture.tsv"),sep="\t",quote="",
  comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(inspection)==7L,nrow(capture)==8L)
causes <- list(missing_first=c(DM="missing"),missing_later=c(AE="missing"),
  directory_first=c(DM="not_regular_file"),directory_later=c(AE="not_regular_file"),
  mixed=c(DM="missing",AE="not_regular_file"),reverse_mixed=c(DM="not_regular_file",AE="missing"),
  all_missing=c(DM="missing",AE="missing",MEDDRA="missing"))
fail_files <- function(case,failures) {
  for(name in names(failures)) {
    path <- file.path(case,paths[[name]])
    stopifnot(unlink(path)==0L)
    if(failures[[name]]=="not_regular_file") dir.create(path)
  }
}
failed <- function(actual,expected,case) {
  stopifnot(identical(rawToChar(observations(actual)),expected),is.null(actual$output),
    identical(names(actual$issues),c("phase","condition","requirement","spec_paths","context")),
    is.character(actual$issues$context))
  invisible(gc())
  for(attempt in seq_len(2L)) {
    refusal <- tryCatch(actual$save(),error=identity)
    stopifnot(inherits(refusal,"yamaa_domain_error"),
      identical(rawToChar(observations(actual)),expected),!file.exists(file.path(case,"adsl.csv")))
  }
}

# Original capture-stage partial/cache reports remain unchanged in typed tests.
# These eight physical configurations exercise public all-source preflight.
for(i in seq_len(nrow(capture))) {
  row <- capture[i,,drop=FALSE]
  kind <- if(row$kind[[1L]]=="missing") "missing" else "directory"
  name <- paste0(kind,if(row$fail_at[[1L]]=="0") "_first" else "_later")
  owner <- new_case()
  fail_files(owner$case,causes[[name]])
  expected <- expected_report(inspection$expected[[match(name,inspection$case)]],owner$entry)
  for(attempt in seq_len(2L)) failed(yamaa_domain(owner$entry),expected,owner$case)
  unlink(owner$directory,recursive=TRUE)
}
cat("eight original physical capture configurations have complete public zero-read failures\n")

for(i in seq_len(nrow(inspection))) {
  owner <- new_case()
  failures <- causes[[inspection$case[[i]]]]
  fail_files(owner$case,failures)
  # Premature study decode would produce a different failure. Every expected
  # metadata condition and its order must survive before any byte capture.
  for(name in setdiff(names(paths),names(failures)))
    writeBin(as.raw(255L),file.path(owner$case,paths[[name]]))
  stopifnot(nrow(yamaa_check(owner$entry)$issues)==0L)
  expected <- expected_report(inspection$expected[[i]],owner$entry)
  for(attempt in seq_len(2L)) failed(yamaa_domain(owner$entry),expected,owner$case)
  unlink(owner$directory,recursive=TRUE)
}
cat("seven complete physical metadata failures repeated before every study read passed\n")

owner <- new_case()
first <- yamaa_domain(owner$entry)
expected <- lookup_report(owner$entry)
stopifnot(identical(rawToChar(observations(first)),expected))
content <- rawfile(file.path(owner$case,paths[["DM"]]))
unlink(file.path(owner$case,paths[["DM"]]))
missing <- expected_report(inspection$expected[[match("missing_first",inspection$case)]],owner$entry)
failed(yamaa_domain(owner$entry),missing,owner$case)
stopifnot(identical(rawToChar(observations(first)),expected))
writeBin(content,file.path(owner$case,paths[["DM"]]))
stopifnot(identical(rawToChar(observations(yamaa_domain(owner$entry))),expected))
unlink(owner$directory,recursive=TRUE)
cat("fresh public builds inspect current physical files and preserve previous owners passed\n")

owner <- new_case()
first <- yamaa_domain(owner$entry)
expected <- lookup_report(owner$entry)
source <- file.path(owner$case,paths[["AE"]])
content <- rawToChar(rawfile(source))
old <- "MYOCARDIAL INFARCTION,FATAL,150\n"
stopifnot(length(gregexpr(old,content,fixed=TRUE)[[1L]])==1L,
  gregexpr(old,content,fixed=TRUE)[[1L]][[1L]]>0L)
writeBin(charToRaw(sub(old,"MYOCARDIAL INFARCTION,FATAL,151\n",content,fixed=TRUE)),source)
second <- yamaa_domain(owner$entry)
stopifnot(nrow(second$issues)==0L,
  identical(as.character(first$output$DTHDY),c("150","80",NA_character_,NA_character_)),
  identical(as.character(second$output$DTHDY),c("151","80",NA_character_,NA_character_)),
  identical(rawToChar(observations(first)),expected),identical(first$save(),TRUE),
  identical(rawfile(file.path(owner$case,"adsl.csv")),rawfile(file.path(owner$case,"expected/adsl.csv"))))
unlink(owner$directory,recursive=TRUE)
cat("changed physical bytes affect fresh builds while prior output remains owned passed\n")

owner <- new_case()
actual <- yamaa_domain(owner$entry)
stopifnot(identical(rawToChar(observations(actual)),lookup_report(owner$entry)),
  identical(as.list(actual$output),public_output_truth[["schema-lookup"]]))
held <- actual$output
golden <- rawfile(file.path(owner$case,"expected/adsl.csv"))
expected <- lookup_report(owner$entry,TRUE)
unlink(owner$entry);unlink(file.path(owner$case,"input"),recursive=TRUE);invisible(gc())
for(attempt in seq_len(2L)) {
  stopifnot(identical(actual$save(),TRUE),identical(rawfile(file.path(owner$case,"adsl.csv")),golden),
    identical(rawToChar(observations(actual)),expected),identical(actual$output,held))
}
rm(actual);invisible(gc())
stopifnot(identical(as.list(held),public_output_truth[["schema-lookup"]]))
unlink(owner$directory,recursive=TRUE)
cat("original complete lookup owner and two exact held Saves after files disappear passed\n")

simple <- function(input) paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n",
  "input: {SRC: ",json_text(input),"}\n",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]\n",
  "output: {path: result.csv, columns: [ID]}\n")
directory <- tempfile("public-local-path-");dir.create(directory)
case <- file.path(directory,"case");dir.create(case)
source <- file.path(directory,"outside.csv")
writeBin(charToRaw("ID\n7\n"),source)
entry <- file.path(case,"spec.yaml")
for(written in c("../outside.csv",normalizePath(source,winslash="/"))) {
  writeBin(charToRaw(simple(written)),entry)
  actual <- yamaa_domain(entry)
  stopifnot(nrow(actual$issues)==0L,identical(as.character(actual$output$ID),"7"),
    identical(actual$save(),TRUE),identical(rawfile(file.path(case,"result.csv")),charToRaw("ID\n7\n")))
}
unlink(directory,recursive=TRUE)
cat("approved parent-relative and absolute public source paths passed\n")

directory <- tempfile("public-link-");dir.create(directory)
writeBin(charToRaw("ID\n7\n"),file.path(directory,"source.csv"))
linked <- suppressWarnings(file.symlink(file.path(directory,"source.csv"),file.path(directory,"alias.csv")))
if(linked) {
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(simple("alias.csv")),entry)
  actual <- yamaa_domain(entry)
  stopifnot(is.null(actual$output),identical(actual$issues$phase,"validation"),
    identical(actual$issues$condition,"resource_path_symlink"),
    identical(actual$issues$requirement,"REQ-0783"),
    identical(actual$issues$spec_paths,list("input.SRC.path")),
    identical(actual$issues$context,'{"dataset":"SRC","path":"alias.csv"}'),
    inherits(tryCatch(actual$save(),error=identity),"yamaa_domain_error"),
    !file.exists(file.path(directory,"result.csv")))
  cat("physical public symlink refusal retains complete native condition passed\n")
} else cat("SKIP: host cannot create physical public symlink control\n")
unlink(directory,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
