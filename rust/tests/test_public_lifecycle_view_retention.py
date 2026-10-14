"""Actual R checkpoint bytes and failure ordering; authored controls are not samples."""

import base64
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"

BOOTSTRAP = r"""
args <- commandArgs(TRUE)
if (!requireNamespace("jsonlite", quietly=TRUE)) quit(status=77L)
expressions <- as.list(parse(args[1]))
e <- new.env(parent=globalenv())
wanted <- c("write_bytes", "write_json", "write_views", "failure")
for (definition in expressions) {
  if (is.call(definition) && identical(definition[[1]], as.name("<-")) &&
      is.symbol(definition[[2]]) && as.character(definition[[2]]) %in% wanted) eval(definition,envir=e)
}
stopifnot(all(vapply(wanted,function(name) exists(name,envir=e,inherits=FALSE),TRUE)))
read_bytes <- function(path) {
  con <- if(endsWith(path,".gz")) gzfile(path,"rb") else file(path,"rb")
  on.exit(close(con))
  readBin(con,"raw",n=1000000L)
}
read_stored_bytes <- function(path) {
  con <- file(path,"rb"); on.exit(close(con))
  readBin(con,"raw",n=1000000L)
}
encode <- function(value) jsonlite::toJSON(value,auto_unbox=TRUE,null="null",na="null",
                                         digits=NA,pretty=TRUE,force=TRUE)
"""


class RCheckpointRetention(unittest.TestCase):
    def run_control(self, body):
        executable = shutil.which("Rscript")
        if executable is None:
            self.skipTest("base R unavailable for actual checkpoint controls")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "control.R"
            script.write_text(BOOTSTRAP + body, encoding="utf-8")
            result = subprocess.run(
                [
                    executable,
                    "--vanilla",
                    str(script),
                    str(TOOLS / "public_lifecycle_r_worker.R"),
                    str(root),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode == 77:
                self.skipTest("explicit jsonlite collector unavailable")
            self.assertEqual(result.returncode, 0, result.stderr)
            record = root / "control.json"
            return (
                json.loads(record.read_text(encoding="utf-8"))
                if record.exists()
                else None
            )

    def test_every_plain_and_gzip_checkpoint_matches_original_bytes_and_field_order(
        self,
    ):
        result = self.run_control(
            r"""
typed <- list(
  list(type="int",value="-9223372036854775808"),
  list(type="int",value="9223372036854775807"),
  list(type="raw",value=as.raw(c(108,101,102,116,0,114,105,103,104,116))),
  list(type="str",value="""
            + '"\u00e9'
            + r""" \\\" quoted"),
  list(type="float",value="3fb999999999999a"),
  list(type="float",value="8000000000000000"),
  list(type="float",value="0000000000000000"),
  list(type="missing",value=NULL),
  list(type="date",value="2026-10-13"),
  list(type="datetime",value="2026-10-13T23:59:58"))
output <- list(columns=lapply(seq_along(typed),function(i) list(paste0("V",i),"typed")),
               row_count=1L,rows=list(typed),host_schema=list())
checkpoints <- list(list(),list(output=output),list(output=output,issues=list(rows=list())),
                    list(output=output,issues=list(rows=list()),verification_log=NULL),
                    list(output=output,issues=list(rows=list()),verification_log=NULL,warning_log=NULL))
# Match all eight reached worker checkpoints, including each log separately.
checkpoints <- c(checkpoints,rep(checkpoints[5],2),list(c(checkpoints[[5]],list(output_after_save_2=output))))
for(suffix in c("",".gz")) {
  cache <- new.env(parent=emptyenv()); count <- 0L
  counted <- function(value) { count <<- count+1L; encode(value) }
  expected_counts <- c(0L,1L,2L,3L,4L,4L,4L,5L)
  for(index in seq_along(checkpoints)) {
    old <- file.path(args[2],paste0("old-",index,".json",suffix))
    new <- file.path(args[2],paste0("new-",index,".json",suffix))
    e$write_json(old,checkpoints[[index]])
    e$write_views(new,checkpoints[[index]],cache,counted)
    stopifnot(identical(read_bytes(old),read_bytes(new)),
              identical(read_stored_bytes(old),read_stored_bytes(new)),
              count==expected_counts[index])
  }
  changed <- checkpoints[[8]]
  changed$output$rows[[1]][[1]]$value <- "9223372036854775807"
  e$write_json(old,changed); e$write_views(new,changed,cache,counted)
  stopifnot(identical(read_bytes(old),read_bytes(new)),count==6L)
  reordered <- changed[rev(names(changed))]
  e$write_json(old,reordered); e$write_views(new,reordered,cache,counted)
  stopifnot(identical(read_bytes(old),read_bytes(new)),count==6L)
}
e$write_json(file.path(args[2],"control.json"),checkpoints[[8]])
"""
        )
        self.assertEqual(
            list(result),
            [
                "output",
                "issues",
                "verification_log",
                "warning_log",
                "output_after_save_2",
            ],
        )
        cells = result["output"]["rows"][0]
        self.assertEqual(base64.b64decode(cells[2]["value"]), b"left\0right")
        self.assertEqual(
            [cell["value"] for cell in cells if cell["type"] != "raw"],
            [
                "-9223372036854775808",
                "9223372036854775807",
                '\u00e9 \\" quoted',
                "3fb999999999999a",
                "8000000000000000",
                "0000000000000000",
                None,
                "2026-10-13",
                "2026-10-13T23:59:58",
            ],
        )

    def test_encoding_error_preserves_previous_file_and_write_error_preserves_primary_cause(
        self,
    ):
        self.run_control(r"""
path <- file.path(args[2],"checkpoint.json")
cache <- new.env(parent=emptyenv())
e$write_views(path,list(output=list(rows=list())),cache)
previous <- read_bytes(path)
opens <- 0L
e$file <- function(...) { opens <<- opens+1L; base::file(...) }
failed <- tryCatch(e$write_views(path,list(output=list(rows=list()),issues=TRUE),cache,
                                function(value) stop("original encoding cause")),error=identity)
stopifnot(identical(conditionMessage(failed),"original encoding cause"),opens==0L,
          identical(read_bytes(path),previous))
write_calls <- close_calls <- 0L; messages <- character()
e$writeBin <- function(...) { write_calls <<- write_calls+1L; stop("original write cause") }
e$close <- function(con) { close_calls <<- close_calls+1L; base::close(con); stop("secondary close cause") }
e$message <- function(...) messages <<- c(messages,paste0(...,collapse=""))
failed <- tryCatch(e$write_views(path,list(output=list(rows=list())),cache),error=identity)
stopifnot(identical(conditionMessage(failed),"original write cause"),write_calls==1L,close_calls==1L,
          any(grepl("secondary close cause",messages,fixed=TRUE)))
""")

    def test_new_view_encoding_failure_in_actual_worker_loop_prevents_every_save(self):
        self.run_control(r"""
loop <- Filter(function(x) is.call(x) && identical(x[[1]],as.name("for")),expressions)
stopifnot(length(loop)==1L)
work <- file.path(args[2],"work");dir.create(work)
e$output <- args[2]
e$request <- list(repetitions=list(list(index=0L,warmup=FALSE,work_directory=work,
                                      output_name="out.csv",artifact_names=list("out.csv"),entry="spec.yaml")),
                  unavailable=list())
e$response <- list(repetitions=list(),failure=NULL)
e$timed <- function(state,name,expr) {
  on.exit(state$durations[[name]] <- list(authored_reached_marker=TRUE));force(expr)
}
metadata_calls <- frame_calls <- issue_calls <- save_calls <- report_calls <- 0L
e$metadata <- function() { metadata_calls <<- metadata_calls+1L; list(synthetic=TRUE) }
e$frame <- function(value) { frame_calls <<- frame_calls+1L; list(rows=list()) }
e$issues <- function(value) { issue_calls <<- issue_calls+1L; list(rows=list()) }
namespace <- new.env(parent=emptyenv())
namespace$.domain_observations <- function(result) { report_calls <<- report_calls+1L; charToRaw('{"original":"build"}') }
e$asNamespace <- function(name) namespace
e$domain <- function(entry) list(output=data.frame(ID=1L),issues=data.frame(),
                                verification_log=NULL,warning_log=NULL,save=function() {
  save_calls <<- save_calls+1L;writeBin(charToRaw("unexpected publication"),file.path(work,"out.csv"));TRUE
})
original <- e$write_views; failed_once <- FALSE
e$write_views <- function(path,value,cache) original(path,value,cache,function(member) {
  if("issues" %in% names(member) && !failed_once) { failed_once <<- TRUE;stop("original issues encoding cause") }
  encode(member)
})
eval(loop[[1]],envir=e)
row <- e$response$repetitions[[1]]
stopifnot(identical(row$failure$message,"original issues encoding cause"),is.null(e$result),
          save_calls==0L,metadata_calls==1L,frame_calls==1L,issue_calls==1L,report_calls==1L,
          !file.exists(file.path(work,"out.csv")),!("held_save_1" %in% names(row$durations)),
          identical(rawToChar(read_bytes(file.path(args[2],row$reports$build))),'{"original":"build"}'))
""")


if __name__ == "__main__":
    unittest.main()
