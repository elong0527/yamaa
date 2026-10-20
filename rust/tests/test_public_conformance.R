# Independently authored lossless public-view serialization truth; base R only.
args <- commandArgs(trailingOnly=TRUE)
stopifnot(length(args)==1L)
source(args[[1L]])
# R exchanges int and float as double: declared types name each column's
# kind, so the same double column encodes exactly as either.
frame <- structure(list(
  I=c(-2^53, 2^53, NA_real_), S=c("", "a\u00e9", NA_character_),
  F=c(1.5,-0.0,NA_real_), D=structure(c(0,1,NA_real_),class="Date"),
  T=structure(c(0,1,NA_real_),class=c("POSIXct","POSIXt"),tzone="UTC")),
  class="data.frame",row.names=c(NA_integer_,-3L))
types <- c(I="int", S="str", F="float", D="date", T="datetime")
actual <- public_table(frame, types)
stopifnot(identical(actual$columns,list(list("I","int"),list("S","str"),list("F","float"),list("D","date"),list("T","datetime"))))
stopifnot(identical(actual$rows[[1L]],list("-9007199254740992","","3ff8000000000000","0","0")))
stopifnot(identical(actual$rows[[2L]],list("9007199254740992","61c3a9","8000000000000000","1","1")))
stopifnot(identical(actual$rows[[3L]],list(NULL,NULL,NULL,NULL,NULL)))
stopifnot(identical(public_table(frame["I"], c(I="float"))$rows[[1L]], list("c340000000000000")),
  is.null(public_table(NULL, NULL)))
# Exactness: an int cell must be a whole double within 2^53 and never
# negative zero; a column's R class must agree with its declared kind.
refused <- function(expr) inherits(tryCatch(expr, error=identity), "error")
stopifnot(refused(public_table(data.frame(I=2.5), c(I="int"))),
  refused(public_table(data.frame(I=2^53+2), c(I="int"))),
  refused(public_table(data.frame(I=-0.0), c(I="int"))),
  refused(public_table(data.frame(I=Inf), c(I="int"))),
  refused(public_table(data.frame(I="1"), c(I="int"))),
  refused(public_table(data.frame(S=1), c(S="str"))),
  refused(public_table(data.frame(D=0), c(D="date"))),
  refused(public_table(data.frame(D=structure(0.5,class="Date")), c(D="date"))),
  refused(public_table(frame["T"], c(T="date"))),
  refused(public_table(data.frame(T=structure(0,class=c("POSIXct","POSIXt"),tzone="America/New_York")), c(T="datetime"))),
  refused(public_table(data.frame(X=1), c(X="bool"))),
  refused(public_table(frame, types[-1L])),
  refused(public_table(frame, rev(types))))
# The run report's fourteen columns: COUNT and TOTAL int, the rest str.
stopifnot(identical(names(report_types), c("ARTIFACT","KIND","SPEC_PATH","NAME","ID","TARGET","RULE",
  "SEVERITY","OUTCOME","CONDITION","COUNT","TOTAL","KEYS","DETAILS")),
  identical(unname(report_types[report_types == "int"]), c("int","int")),
  identical(names(report_types)[report_types == "int"], c("COUNT","TOTAL")))
# Output types come from the resolved entry, never from a producer.
resolved <- list(entry="/study/root.yaml", entries=list(
  list(entry="/study/producer.yaml", specification=list(columns=list(list(name="ID", type="str")))),
  list(entry="/study/root.yaml", specification=list(columns=list(
    list(name="ID", type="int"), list(name="V", type="float"), list(name="X", type="date"))))))
spec <- list(resolve=function() resolved)
stopifnot(identical(declared_types(spec, c("V","ID")), c(V="float", ID="int")),
  refused(declared_types(spec, c("ID","MISSING"))),
  refused(declared_types(list(resolve=function() list(entry="/other.yaml", entries=resolved$entries)), "ID")))
stopifnot(identical(json(list(issues=list(),paths=list("a"),context=raw_json('{"k":1}'))),'{"issues":[],"paths":["a"],"context":{"k":1}}'))
# relative_paths: a record keeps the paths a save returned relative to the
# case's root; a path outside the case, or with a backslash, stays as
# returned. Only Windows compares the root without letter case.
stopifnot(identical(relative_paths(c("/w/case/report.csv", "/w/case/out/adsl.csv"), "/w/case"),
    c("report.csv", "out/adsl.csv")),
  identical(relative_paths(character(), "/w/case"), character()),
  identical(relative_paths(c("/w/case2/x.csv", "/w/x.csv", "/w/case/out\\x.csv"), "/w/case"),
    c("/w/case2/x.csv", "/w/x.csv", "/w/case/out\\x.csv")),
  identical(relative_paths("/W/case/x.csv", "/w/case"),
    if (.Platform$OS.type == "windows") "x.csv" else "/W/case/x.csv"))
stopifnot(identical(json_text("a\n\"\\"),'"a\\n\\\"\\\\"'))
stopifnot(identical(hex(charToRaw(enc2utf8("\u00e9"))),"c3a9"))
# move_inputs: a rename that fails (as it does across filesystems) must fall
# back to copy-and-delete, and a missing source must still stop.
move_root <- tempfile("move-inputs-")
staged <- file.path(move_root, "staging", "case")
dir.create(file.path(staged, "nested"), recursive=TRUE, showWarnings=FALSE)
dir.create(file.path(staged, "empty"), showWarnings=FALSE)
top_bytes <- c(charToRaw("top"), as.raw(0L), charToRaw("bytes"))
writeBin(top_bytes, file.path(staged, "top.bin"))
writeBin(charToRaw("nested bytes"), file.path(staged, "nested", "data.bin"))
moved <- file.path(move_root, "work", "case")
file.rename <- function(from, to) FALSE
move_inputs(staged, moved)
rm(file.rename)
stopifnot(!dir.exists(staged))
stopifnot(identical(readBin(file.path(moved, "top.bin"), "raw", 16L), top_bytes))
stopifnot(identical(readBin(file.path(moved, "nested", "data.bin"), "raw", 16L), charToRaw("nested bytes")))
stopifnot(dir.exists(file.path(moved, "empty")))
# Staged trees preserve symbolic links, so the fallback must recreate them
# as links (broken ones included) rather than following them.
if (.Platform$OS.type == "unix") {
  link_root <- tempfile("move-inputs-links-")
  link_staged <- file.path(link_root, "staging", "case")
  dir.create(file.path(link_staged, "realdir"), recursive=TRUE, showWarnings=FALSE)
  writeBin(charToRaw("linked"), file.path(link_staged, "realdir", "data.bin"))
  stopifnot(file.symlink("realdir", file.path(link_staged, "dirlink")))
  stopifnot(file.symlink(file.path("realdir", "data.bin"), file.path(link_staged, "filelink")))
  stopifnot(file.symlink("absent-target", file.path(link_staged, "brokenlink")))
  link_moved <- file.path(link_root, "work", "case")
  file.rename <- function(from, to) FALSE
  move_inputs(link_staged, link_moved)
  rm(file.rename)
  stopifnot(!dir.exists(link_staged))
  stopifnot(identical(unname(Sys.readlink(file.path(link_moved, "dirlink"))), "realdir"))
  stopifnot(identical(unname(Sys.readlink(file.path(link_moved, "filelink"))), file.path("realdir", "data.bin")))
  stopifnot(identical(unname(Sys.readlink(file.path(link_moved, "brokenlink"))), "absent-target"))
  stopifnot(identical(readBin(file.path(link_moved, "realdir", "data.bin"), "raw", 16L), charToRaw("linked")))
  unlink(link_root, recursive=TRUE, force=TRUE)
}
stopifnot(inherits(try(move_inputs(file.path(move_root, "absent"), moved), silent=TRUE), "try-error"))
unlink(move_root, recursive=TRUE, force=TRUE)
# read_plan: optional columns that are blank on every row (one case without an
# environment, exclusion or variant) must stay "", not become logical NA, so
# the runner keeps the case as a runnable entry.
plan_root <- tempfile("plan-")
dir.create(plan_root)
plan_header <- "example\tspecification\tenvironment\texcluded\tvariant\tentry"
writeLines(c(plan_header, "case-a\tspec.yaml\t\t\t\t"), file.path(plan_root, "r.tsv"))
plan <- read_plan(plan_root)
stopifnot(all(vapply(plan, is.character, logical(1L))))
stopifnot(identical(unlist(plan[1L, ], use.names=FALSE), c("case-a", "spec.yaml", "", "", "", "")))
plan_entries <- plan[!nzchar(plan$variant), , drop=FALSE]
stopifnot(identical(nrow(plan_entries), 1L), !nzchar(plan_entries$excluded[[1L]]), !nzchar(plan_entries$environment[[1L]]))
# Filled columns are read as the same text, including the variant entry index.
writeLines(c(plan_header, "case-b\tspec.yaml\tr/environment.yaml\t\t\t",
  "case-b\t__child_spec.yaml\tr/environment.yaml\t\tempty_child\t0"), file.path(plan_root, "r.tsv"))
plan <- read_plan(plan_root)
stopifnot(all(vapply(plan, is.character, logical(1L))), identical(plan$entry, c("", "0")),
  identical(plan$variant, c("", "empty_child")), identical(plan$environment, rep("r/environment.yaml", 2L)))
unlink(plan_root, recursive=TRUE, force=TRUE)
# observed_plan: the measurements close the observations, so their plan is the
# last one named, after a plan named earlier and however many characters
# outside the Basic Multilingual Plane come before it. R's default regular
# expressions count each such character as two UTF-16 code units on Windows,
# so there this checks that the match and the slice count alike.
observations <- function(plan) paste0('{"protocol":"held-observations/1","report":{"tables":[{"value":"',
  strrep(paste0(intToUtf8(0x1F600L), "\u00e9"), 42L), '"}],"plan":"compiled"},"measurements":{"plan":"',
  plan, '","protocol":"application-measurements/1","stages":[]}}')
stopifnot(identical(observed_plan(observations("compiled")), "compiled"),
  identical(observed_plan(observations("reused")), "reused"),
  identical(observed_plan(observations("refused")), "refused"),
  is.null(observed_plan('{"protocol":"held-observations/1","report":null,"measurements":{}}')))
cat("lossless R public tables typed by declaration, exact int and temporal cells, float bits and issue JSON passed\n")
cat("relative_paths records saved paths below the case's root passed\n")
cat("move_inputs cross-filesystem copy-and-delete fallback passed\n")
cat("read_plan keeps blank and filled plan columns as text passed\n")
cat("observed_plan reads the measurements' plan after characters outside the BMP passed\n")
