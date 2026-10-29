# Installed original specifications: the public domain cases' complete reports,
# typed outputs, issues and exact saved bytes, a check without study data,
# full-range integers, retained save retries and project refusals, and saves
# through linked parents. The shared helpers' own controls run here too. The
# other families are original_specifications_<family>.R; Python reads the same
# truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# The comparisons the suites rely on refuse what they must.
local({
  cases <- list(
    list(label="an issue without spec paths", paths=character(), written='[]'),
    list(label="an issue with spec paths", paths=c("columns.ID", "keys[0]"),
      written='["columns.ID","keys[0]"]'))
  for (case in cases) {
    expected <- jsonlite::fromJSON(paste0('[{"phase":"validation","condition":"unknown_field",',
      '"rule":"NAME-3","spec_paths":',case$written,',"context":{}}]'),simplifyVector=FALSE)
    actual <- data.frame(phase="validation",condition="unknown_field",rule="NAME-3",context="{}")
    actual$spec_paths <- list(case$paths)
    expect_issue_rows(actual,expected,case$label)
    actual$spec_paths <- list("output")
    failure <- tryCatch(expect_issue_rows(actual,expected,case$label),error=identity)
    stopifnot_case(case$label,"changed spec paths are refused"=inherits(failure,"error"),
      "the refusal names the case"=startsWith(conditionMessage(failure),case$label))
  }
})
local({
  decode <- function(text) jsonlite::fromJSON(text,simplifyVector=FALSE)
  actual <- decode('{"keys":[{"B":2,"A":1},null],"dataset":"SRC"}')
  stopifnot("contexts ignore object key order"=context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":1,"B":2},null]}')),
    "contexts keep array order"=!context_equal(actual,decode('{"dataset":"SRC","keys":[null,{"A":1,"B":2}]}')),
    "contexts keep scalar types"=!context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":"1","B":2},null]}')),
    "contexts keep every element"=!context_equal(actual,decode('{"dataset":"SRC","keys":[{"A":1,"B":2}]}')))
})
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
local({
  wire <- '{"minimum":-9223372036854775808,"maximum":9223372036854775807}'
  adjacent <- sub("9223372036854775807","9223372036854775806",wire,fixed=TRUE)
  stopifnot("the adjacent wire text differs"=!identical(wire,adjacent),
    "wide numbers are refused rather than compared as doubles"=
      inherits(tryCatch(json_equal(wire,jsonlite::fromJSON(adjacent,simplifyVector=FALSE)),error=identity),"error"))
})
local({
  expected <- '[{"spec_path":"rows[1].V","handler":"multiple_matches","count":2}]'
  observed <- '[{"count":2,"handler":"multiple_matches","spec_path":"rows[1].V"}]'
  report <- function(value) paste0('{"handler_counts":',value,',"node":{"handler_counts":',value,'}}')
  check_handler_ledger(report(observed),expected,"a ledger in another key order")
  for(value in c(sub('"count":2','"count":1',observed,fixed=TRUE),
    sub(',"handler":"multiple_matches"','',observed,fixed=TRUE),
    sub('"count":2','"count":2,"extra":0',observed,fixed=TRUE),
    sub('"count":2','"count":2,"count":2',observed,fixed=TRUE),
    paste0('[',substring(observed,2L,nchar(observed)-1L),',',substring(observed,2L,nchar(observed)-1L),']'))) {
    stopifnot_case(value,"a changed ledger is refused"=
      inherits(tryCatch(check_handler_ledger(report(value),expected),error=identity),"error"))
  }
  ordered <- '[{"count":1,"handler":"no_match","spec_path":"A"},{"count":2,"handler":"no_match","spec_path":"B"}]'
  reversed <- '[{"count":2,"handler":"no_match","spec_path":"B"},{"count":1,"handler":"no_match","spec_path":"A"}]'
  stopifnot("a reordered ledger is refused"=
    inherits(tryCatch(check_handler_ledger(report(reversed),ordered),error=identity),"error"))
})

# Staged int truth is decimal text, here or under the retired int64 class; R
# exchanges int as double, so convert it where the conversion is exact.
typed_truth <- function(expected,types,label) {
  expect_identical(length(expected),length(types),paste(label,"types every column"))
  for(index in seq_along(expected)) if(identical(types[[index]],"int")) {
    column <- expected[[index]]
    if(is.double(column) && is.null(attributes(column))) next
    text <- as.character(unclass(column))
    value <- as.double(text)
    expect_identical(integer_text(value),text,paste(label,"column",index,"converts exactly"))
    expected[[index]] <- value
  }
  expected
}
artifact_types <- function(case_name) {
  report <- jsonlite::fromJSON(file.path(root,"expected",paste0(case_name,".json")),simplifyVector=FALSE)
  fixture_strings(report$artifacts[[1L]]$types)
}
source(file.path(root,"issue-frame-truth.R"),local=TRUE)
source(file.path(root,"public-output-truth.R"),local=TRUE)
for(case_name in names(issue_frame_truth)[!startsWith(names(issue_frame_truth),"static/")]) {
  directory <- tempfile("public-domain-");dir.create(directory)
  stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml")
  built <- run_spec(entry)
  report <- r_report_text(rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json")))),case_name,entry,
    basename(entry))
  entry_field <- paste0('"specification":',json_text(entry))
  unsaved <- unsaved_report_text(report)
  expect_identical(domain_observations(built),unsaved,paste(case_name,"has the authored unsaved report"))
  check_issue_frame(built$issues,issue_frame_truth[[case_name]],paste("the issues of",case_name))
  check_run_report(built,paste("the run report of",case_name))
  expected <- public_output_truth[[case_name]]
  if(!is.null(expected)) expected <- typed_truth(expected,artifact_types(case_name),paste("the output truth of",case_name))
  if(is.null(expected)) {
    stopifnot_case(case_name,"a failed case has no output"=is.null(built$output),
      "its save is refused"=save_refused(built))
    expect_identical(domain_observations(built),unsaved,paste(case_name,"keeps its unsaved report"))
  } else {
    stopifnot_case(case_name,"the output is a data frame"=is.data.frame(built$output))
    expect_identical(as.list(built$output),expected,paste(case_name,"has the authored typed output"))
    output_name <- switch(case_name,"schema-lookup"="adsl.csv","schema-window-functions"="advs.csv","adlb.csv")
    for(attempt in seq_len(2L)) {
      label <- sprintf("%s save %d",case_name,attempt)
      stopifnot_case(label,"the run saves"=saved(built))
      expect_identical(rawfile(file.path(case,output_name)),rawfile(file.path(case,"expected",output_name)),
        paste(label,"writes the authored bytes"))
      expect_identical(domain_observations(built),report,paste(label,"records the authored report"))
    }
    expect_identical(as.list(built$output),expected,paste(case_name,"keeps its typed output"))
  }
  report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  if(nzchar(report_directory)) {
    # Relabel only the observed run identity; semantic findings and reads stay exact.
    observed <- domain_observations(built)
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
writeLines(c('yamaa: "1.0"',"type: spec","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
             "output: {path: absent/output.csv, columns: [ID]}","columns:",
             "  - {name: ID, type: int, derivation: SRC.ID}"),entry)
stopifnot("a check reads no study data"=nrow(check_spec(entry)$issues)==0L)
failed <- run_spec(entry)
expect_identical(failed$issues$condition,"resource_path_missing","a run without its input")
stopifnot("a run without its input has no output"=is.null(failed$output),"its save is refused"=save_refused(failed))
writeBin(fixture_bytes(host_truth[["integer_boundary_csv"]]),file.path(directory,"input.csv"))
built <- run_spec(entry)
# R exchanges int as double: the output view refuses the int64 extremes rather
# than change them, while the held run saves them exactly below.
unheld <- tryCatch(built$output,error=identity)
stopifnot("the full-range run has no issues"=nrow(built$issues)==0L,
  "its output view signals yamaa_error"=inherits(unheld,"yamaa_error"))
expect_identical(conditionMessage(unheld),"integer -9223372036854775808 is beyond 2^53 and cannot be held exactly by an R double",
  "the view names the first unheld integer")
expect_identical(built$report$COUNT[[1L]],2,"the run counts both rows")
writeLines("[changed",entry); writeLines("changed",file.path(directory,"input.csv"))
# The output directory does not exist (FILE-7).
stopifnot("a save into a missing directory is refused"=save_refused(built))
expect_identical(built$issues$condition,"resource_path_missing","a save into a missing directory")
dir.create(file.path(directory,"absent"))
stopifnot("the retry saves"=saved(built),"the retry leaves no issues"=nrow(built$issues)==0L)
expect_identical(rawfile(file.path(directory,"absent/output.csv")),fixture_bytes(host_truth[["integer_boundary_csv"]]),
  "the retry saves the full-range integers exactly")
# A missing project file refuses every project operation before any
# specification is read: the run builds nothing and its save signals.
absent <- yamaa_project("absent-project.yaml")
refused <- absent$run()
expect_identical(refused$names,character(),"an absent project builds nothing")
expect_identical(names(refused$report),report_columns,"an absent project's report columns")
expect_identical(nrow(refused$report),0L,"an absent project's report is empty")
expect_identical(as.list(refused$issues[c("phase","condition","rule","spec_paths","context")]),
  list(phase="validation",condition="resource_path_missing",rule="FILE-7",spec_paths=list(character()),
    context='{"path":"absent-project.yaml"}'),
  "an absent project's one issue")
expect_identical(refused$issues,absent$check()$issues,"an absent project's check")
stopifnot("an absent project's save is refused"=save_refused(refused))
# The caller's own path, named as given at no field (FILE-7).
unread <- tryCatch(absent$spec("TEST"),error=identity)
stopifnot("selecting from an absent project signals yamaa_error"=inherits(unread,"yamaa_error"))
expect_identical(conditionMessage(unread),paste0("cannot select TEST: ",
  "absent-project.yaml does not exist [resource_path_missing, FILE-7]"),"selecting from an absent project")
expect_identical(unread$issues,refused$issues,"the selection carries the project's issue")
# Argument mistakes signal yamaa_error.
argument <- tryCatch(yamaa_spec(NULL)$run(),error=identity)
stopifnot("an argument mistake is a package error"=inherits(argument,"error") && inherits(argument,"yamaa_error"))
unlink(directory,recursive=TRUE)
cat("public check without study data, full-range integers, retained save retry and project refusal passed\n")

remove_directory_link <- function(path) {
  if(.Platform$OS.type=="windows") {
    # R's unlink treats directory symbolic links as mount-point reparse tags.
    # rmdir without /s removes only this link, preserving its external target.
    command <- Sys.getenv("COMSPEC")
    stopifnot("the command interpreter exists"=nzchar(command) && file.exists(command))
    system2(command,c("/d","/c","rmdir",shQuote(chartr("/","\\",path),type="cmd")))
  } else unlink(path,recursive=TRUE)
}
for(late in c(FALSE,TRUE)) for(existing in c(FALSE,TRUE)) {
  label <- sprintf("a link %s the build%s",if(late) "made after" else "made before",
    if(existing) ", over an existing target" else "")
  directory <- tempfile("public-linked-output-");dir.create(directory)
  outside <- tempfile("public-output-outside-");dir.create(outside)
  sentinel <- file.path(outside,"sentinel")
  sentinel_bytes <- charToRaw("outside sentinel")
  writeBin(sentinel_bytes,sentinel)
  entry <- file.path(directory,"spec.yaml")
  writeLines(c('yamaa: "1.0"',"type: spec","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
               "output: {path: link/output.csv, columns: [ID]}","columns:",
               "  - {name: ID, type: int, derivation: SRC.ID}"),entry)
  writeBin(charToRaw("ID\n1\n"),file.path(directory,"input.csv"))
  target <- file.path(outside,"output.csv")
  if(existing) writeBin(charToRaw("retained"),target)
  link <- file.path(directory,"link")
  if(late) dir.create(link) else stopifnot_case(label,"the link is made"=file.symlink(outside,link))
  built <- run_spec(entry)
  expect_identical(as.character(built$output$ID),"1",paste(label,"builds its row"))
  if(late) {
    stopifnot_case(label,"the directory is replaced by the link"=
      identical(unlink(link,recursive=TRUE),0L) && !file.exists(link) && file.symlink(outside,link))
  }
  # A link on the way to the target (FILE-8).
  stopifnot_case(label,"the save is refused"=save_refused(built))
  expect_identical(as.list(built$issues[c("condition","context")]),
    list(condition="resource_path_symlink",context='{"path":"link/output.csv"}'),paste(label,"is refused at the link"))
  expect_identical(list.files(outside,all.files=TRUE,no..=TRUE),if(existing) c("output.csv","sentinel") else "sentinel",
    paste(label,"writes nothing outside"))
  expect_identical(rawfile(sentinel),sentinel_bytes,paste(label,"leaves the outside sentinel"))
  if(existing) expect_identical(rawfile(target),charToRaw("retained"),paste(label,"leaves the outside target"))
  stopifnot_case(label,"the link is removed alone"=identical(remove_directory_link(link),0L) && !file.exists(link))
  expect_identical(rawfile(sentinel),sentinel_bytes,paste(label,"keeps the sentinel after the link goes"))
  if(existing) expect_identical(rawfile(target),charToRaw("retained"),paste(label,"keeps the target after the link goes"))
  stopifnot_case(label,"a real directory takes the link's place"=dir.create(link),
    "the retry saves"=saved(built),"the retry leaves no issues"=nrow(built$issues)==0L)
  expect_identical(rawfile(file.path(link,"output.csv")),charToRaw("ID\n1\n"),paste(label,"retries into the directory"))
  expect_identical(rawfile(sentinel),sentinel_bytes,paste(label,"never writes outside"))
  unlink(directory,recursive=TRUE);unlink(outside,recursive=TRUE)
}
cat("public save rejects linked parents before or after build, retains external targets and permits safe retry passed\n")

restore_path()
