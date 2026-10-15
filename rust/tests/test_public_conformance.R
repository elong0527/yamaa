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
writeBin(charToRaw("top\0bytes"), file.path(staged, "top.bin"))
writeBin(charToRaw("nested bytes"), file.path(staged, "nested", "data.bin"))
moved <- file.path(move_root, "work", "case")
file.rename <- function(from, to) FALSE
move_inputs(staged, moved)
rm(file.rename)
stopifnot(!dir.exists(staged))
stopifnot(identical(readBin(file.path(moved, "top.bin"), "raw", 16L), charToRaw("top\0bytes")))
stopifnot(identical(readBin(file.path(moved, "nested", "data.bin"), "raw", 16L), charToRaw("nested bytes")))
stopifnot(dir.exists(file.path(moved, "empty")))
stopifnot(inherits(try(move_inputs(file.path(move_root, "absent"), moved), silent=TRUE), "try-error"))
unlink(move_root, recursive=TRUE, force=TRUE)
cat("lossless R public tables, NUL text, signed int64, float bits, temporal values and issue JSON passed\n")
cat("move_inputs cross-filesystem copy-and-delete fallback passed\n")
