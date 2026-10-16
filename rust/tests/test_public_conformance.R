# Independently authored lossless public-view serialization truth; base R only.
args <- commandArgs(trailingOnly=TRUE)
stopifnot(length(args)==1L)
source(args[[1L]])
frame <- structure(list(
  I=structure(c("-9223372036854775808", "9223372036854775807", NA_character_), class="yamaa_int64_vector"),
  S=structure(list(raw(), as.raw(c(97L,0L,98L)), NULL), class="yamaa_utf8_vector"),
  F=c(1.5,-0.0,NA_real_), D=structure(c(0,1,NA_real_),class="Date"),
  T=structure(c(0,1,NA_real_),class=c("POSIXct","POSIXt"),tzone="UTC")),
  class="data.frame",row.names=c(NA_integer_,-3L))
actual <- public_table(frame)
stopifnot(identical(actual$columns,list(list("I","int"),list("S","str"),list("F","float"),list("D","date"),list("T","datetime"))))
stopifnot(identical(actual$rows[[1L]],list("-9223372036854775808","","3ff8000000000000","0","0")))
stopifnot(identical(actual$rows[[2L]],list("9223372036854775807","610062","8000000000000000","1","1")))
stopifnot(identical(actual$rows[[3L]],list(NULL,NULL,NULL,NULL,NULL)))
stopifnot(identical(json(list(issues=list(),paths=list("a"),context=raw_json('{"k":1}'))),'{"issues":[],"paths":["a"],"context":{"k":1}}'))
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
cat("lossless R public tables, NUL text, signed int64, float bits, temporal values and issue JSON passed\n")
cat("move_inputs cross-filesystem copy-and-delete fallback passed\n")
cat("read_plan keeps blank and filled plan columns as text passed\n")
