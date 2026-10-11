"""Actual worker failure paths with synthetic owners/clocks; no candidate samples."""

import builtins
import inspect
import io
import itertools
import json
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

from test_public_lifecycle_measurement import TOOLS, controller, fresh_worker_imports, worker


class ReachedEvidence(unittest.TestCase):
    def test_python_final_response_failure_releases_owner_and_preserves_cause(self):
        for interrupted in (False, True):
            with (
                self.subTest(interrupted=interrupted),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                work = root / "work"
                work.mkdir()
                reached = {}

                class Owner:
                    output = issues = object()
                    verification_log = warning_log = None

                    def __init__(self):
                        self.state = "build"
                        self._native = types.SimpleNamespace(
                            observations=lambda: json.dumps({"owner": self.state})
                        )

                    def save(self):
                        self.state = "failed-publication"
                        (work / "out.csv").write_bytes(b"independent completed prefix")
                        if interrupted:
                            raise KeyboardInterrupt("independent original interruption")
                        raise ValueError("independent original Save cause")

                candidate = types.ModuleType("yamaa")
                candidate.domain = lambda entry: Owner()
                real_import = builtins.__import__
                real_write = worker.write_json

                def control_import(name, *args, **kwargs):
                    if name == "yamaa":
                        return candidate
                    if name == "polars":
                        return types.ModuleType("polars")
                    return real_import(name, *args, **kwargs)

                def write(path, value):
                    if path.name == "response.json" and any(
                        row["failure"] is not None for row in value["repetitions"]
                    ):
                        reached["response"] = value
                        frame = inspect.currentframe().f_back
                        while frame.f_code is not worker.run.__code__:
                            frame = frame.f_back
                        reached["frame"] = frame
                        raise OSError(
                            "independent secondary response transport failure"
                        )
                    return real_write(path, value)

                declaration = {
                    "index": 0,
                    "warmup": True,
                    "work_directory": str(work),
                    "entry": str(work / "spec.yaml"),
                    "output_name": "out.csv",
                }
                request = {
                    "output": str(root),
                    "protocol": "public-lifecycle/1",
                    "child": "final-response-control",
                    "unavailable": controller.UNAVAILABLE,
                    "repetitions": [declaration, dict(declaration, index=1)],
                }
                stderr = io.StringIO()
                with (
                    fresh_worker_imports(),
                    mock.patch("builtins.__import__", side_effect=control_import),
                    mock.patch.object(worker, "write_json", side_effect=write),
                    mock.patch.object(worker, "metadata", return_value={}),
                    mock.patch.object(worker, "frame", return_value={}),
                    mock.patch.object(worker, "issues", return_value={}),
                    mock.patch.object(worker, "rss", return_value=None),
                    mock.patch.object(
                        worker.time,
                        "perf_counter_ns",
                        side_effect=itertools.count(100, 10),
                    ),
                    mock.patch.object(sys, "stderr", new=stderr),
                ):
                    self.assertEqual(worker.run(request), 1)
                self.assertIsNone(reached["frame"].f_locals["result"])
                response = reached["response"]
                self.assertEqual(len(response["repetitions"]), 1)
                row = response["repetitions"][0]
                primary = (
                    "independent original interruption"
                    if interrupted
                    else "independent original Save cause"
                )
                self.assertEqual(row["failure"]["message"], primary)
                self.assertEqual(response["failure"], row["failure"])
                self.assertIn(primary, stderr.getvalue())
                self.assertIn(
                    "independent secondary response transport failure",
                    stderr.getvalue(),
                )
                self.assertEqual(
                    row["evidence_capture_failures"][0]["location"], "final_response"
                )
                self.assertNotIn("held_save_2", row["durations"])
                self.assertEqual(
                    (root / row["saved_files"]["save_1"]).read_bytes(),
                    b"independent completed prefix",
                )
                self.assertEqual(
                    json.loads((root / row["reports"]["save_1"]).read_text()),
                    {"owner": "failed-publication"},
                )
                partial = json.loads((root / "response.json").read_text())
                self.assertIsNone(partial["repetitions"][0]["failure"])
                del reached["frame"]

    def test_python_metadata_save_secondary_and_interrupt_preserve_reached_owner(self):
        for mode in ("metadata", "save", "secondary", "interrupt"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                owners = []

                class Owner:
                    output = object()
                    issues = object()
                    verification_log = warning_log = None

                    def __init__(self, entry):
                        self.target = Path(entry).parent / "output.csv"
                        self.state = "build"

                        def observations():
                            if mode == "secondary" and self.state != "build":
                                raise OSError("secondary observation transport cause")
                            return json.dumps({"independent_owner_state": self.state})

                        self._native = types.SimpleNamespace(observations=observations)

                    def save(self):
                        self.state = "failed-publication"
                        self.target.write_bytes(b"independent completed prefix")
                        if mode == "interrupt":
                            raise KeyboardInterrupt("original Save interruption")
                        raise ValueError("original late Save cause")

                def build(entry):
                    owner = Owner(entry)
                    owners.append(owner)
                    return owner

                candidate = types.ModuleType("yamaa")
                candidate.domain = build
                real_import = builtins.__import__

                def control_import(name, *args, **kwargs):
                    if name == "yamaa":
                        return candidate
                    if name == "polars":
                        return types.ModuleType("polars")
                    return real_import(name, *args, **kwargs)

                def metadata(_):
                    if mode == "metadata":
                        raise LookupError("original metadata cause")
                    return {"synthetic": True}

                declarations = []
                for index in range(2):
                    work = root / f"work-{index}"
                    work.mkdir()
                    declarations.append(
                        {
                            "index": index,
                            "warmup": index == 0,
                            "work_directory": str(work),
                            "entry": str(work / "spec.yaml"),
                            "output_name": "output.csv",
                        }
                    )
                request = {
                    "output": str(root),
                    "protocol": "public-lifecycle/1",
                    "child": "failure-control",
                    "repetitions": declarations,
                    "unavailable": controller.UNAVAILABLE,
                }
                with (
                    fresh_worker_imports(),
                    mock.patch("builtins.__import__", side_effect=control_import),
                    mock.patch.object(worker, "metadata", side_effect=metadata),
                    mock.patch.object(
                        worker, "frame", return_value={"synthetic": True}
                    ),
                    mock.patch.object(
                        worker, "issues", return_value={"synthetic": True}
                    ),
                    mock.patch.object(
                        worker.time,
                        "perf_counter_ns",
                        side_effect=itertools.count(100, 10),
                    ),
                    mock.patch.object(sys, "stderr", new=io.StringIO()),
                ):
                    self.assertEqual(worker.run(request), 1)
                response = json.loads((root / "response.json").read_text())
                self.assertEqual(
                    len(response["repetitions"]), 1 if mode == "interrupt" else 2
                )
                self.assertEqual(len(owners), len(response["repetitions"]))
                for row in response["repetitions"]:
                    self.assertEqual(row["status"], "failure")
                    self.assertEqual(
                        json.loads((root / row["reports"]["build"]).read_text()),
                        {"independent_owner_state": "build"},
                    )
                    self.assertIn("public_build", row["durations"])
                    self.assertIn("metadata_after_build", row["durations"])
                    if mode == "metadata":
                        self.assertEqual(
                            row["failure"]["message"], "original metadata cause"
                        )
                        self.assertNotIn("held_save_1", row["durations"])
                        continue
                    self.assertEqual(
                        row["failure"]["message"],
                        "original Save interruption"
                        if mode == "interrupt"
                        else "original late Save cause",
                    )
                    self.assertIn("held_save_1", row["durations"])
                    self.assertNotIn("held_save_2", row["durations"])
                    self.assertEqual(
                        (root / row["saved_files"]["save_1"]).read_bytes(),
                        b"independent completed prefix",
                    )
                    if mode == "secondary":
                        self.assertNotIn("save_1", row["reports"])
                        self.assertEqual(
                            row["evidence_capture_failures"][0]["message"],
                            "secondary observation transport cause",
                        )
                    else:
                        self.assertEqual(
                            json.loads((root / row["reports"]["save_1"]).read_text()),
                            {"independent_owner_state": "failed-publication"},
                        )

    def test_r_actual_repetition_ast_metadata_save_and_secondary_failure_retention(
        self,
    ):
        executable = shutil.which("Rscript")
        if executable is None:
            self.skipTest("base R unavailable for synthetic owner control")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "failure-control.R"
            script.write_text(
                """
args <- commandArgs(TRUE)
expressions <- as.list(parse(args[1]))
loop <- Filter(function(x) is.call(x) && identical(x[[1]], as.name("for")), expressions)
f <- Filter(function(x) is.call(x) && identical(x[[1]], as.name("<-")) &&
            identical(x[[2]], as.name("failure")), expressions)
stopifnot(length(loop)==1L, length(f)==1L)
for (mode in c("metadata", "save", "secondary")) {
  directory <- file.path(args[2], mode); dir.create(directory)
  work <- file.path(directory,"work"); dir.create(work)
  e <- new.env(parent=globalenv()); stores <- new.env(parent=emptyenv())
  owner <- new.env(parent=emptyenv()); owner$state <- "build"
  e$output <- directory
  e$request <- list(repetitions=list(list(index=0L, work_directory=work,
                                        output_name="out.csv", entry=file.path(work,"spec.yaml"))), unavailable=list())
  e$response <- list(repetitions=list(),failure=NULL)
  e$write_json <- function(path,value) assign(basename(path), value, envir=stores)
  e$timed <- function(state,name,expr) {
    on.exit(state$durations[[name]] <- list(authored_reached_marker=TRUE))
    force(expr)
  }
  e$frame <- function(value) if(is.null(value)) NULL else list(synthetic=TRUE)
  e$issues <- function(value) list(synthetic=TRUE)
  namespace <- new.env(parent=emptyenv())
  namespace$.domain_observations <- function(result) {
    if(mode=="secondary" && owner$state!="build") stop("secondary observation transport cause")
    charToRaw(paste0('{"independent_owner_state":"',owner$state,'"}'))
  }
  e$asNamespace <- function(name) namespace
  e$metadata <- function() if(mode=="metadata") stop("original metadata cause") else list(synthetic=TRUE)
  e$domain <- function(entry) list(output=data.frame(ID=1L), issues=data.frame(),
                                 verification_log=NULL,warning_log=NULL,
                                 save=function() {
    owner$state <- "failed-publication"
    writeBin(charToRaw("independent completed prefix"), file.path(work,"out.csv"))
    stop("original late Save cause")
  })
  eval(f[[1]], envir=e)
  eval(loop[[1]], envir=e)
  row <- get("response.json", envir=stores)$repetitions[[1]]
  stopifnot(identical(row$status,"failure"), is.null(e$result),
            identical(rawToChar(readBin(file.path(directory,row$reports$build),"raw",n=1000)),
                      '{"independent_owner_state":"build"}'),
            "public_build" %in% names(row$durations), "metadata_after_build" %in% names(row$durations))
  if(mode=="metadata") {
    stopifnot(identical(row$failure$message,"original metadata cause"),
              !("held_save_1" %in% names(row$durations)))
  } else {
    stopifnot(identical(row$failure$message,"original late Save cause"),
              "held_save_1" %in% names(row$durations), !("held_save_2" %in% names(row$durations)),
              identical(rawToChar(readBin(file.path(directory,row$saved_files$save_1),"raw",n=1000)),
                        "independent completed prefix"))
    if(mode=="secondary") {
      stopifnot(is.null(row$reports$save_1),
                identical(row$evidence_capture_failures[[1]]$message,"secondary observation transport cause"))
    } else {
      stopifnot(identical(rawToChar(readBin(file.path(directory,row$reports$save_1),"raw",n=1000)),
                        '{"independent_owner_state":"failed-publication"}'))
    }
  }
}
cat("actual R repetition AST independent failure controls PASS\\n")
""",
                encoding="utf-8",
            )
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
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("failure controls PASS", result.stdout)

    def test_r_final_response_failure_releases_owner_and_preserves_cause(self):
        executable = shutil.which("Rscript")
        if executable is None:
            self.skipTest("base R unavailable for synthetic owner control")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "final-response-control.R"
            script.write_text(
                """
args <- commandArgs(TRUE)
expressions <- as.list(parse(args[1]))
loop <- Filter(function(x) is.call(x) && identical(x[[1]], as.name("for")), expressions)
helper <- Filter(function(x) is.call(x) && identical(x[[1]], as.name("<-")) &&
                 identical(x[[2]], as.name("failure")), expressions)
stopifnot(length(loop)==1L, length(helper)==1L)
for (interrupted in c(FALSE, TRUE)) {
  directory <- file.path(args[2], as.character(interrupted)); dir.create(directory)
  work <- file.path(directory, "work"); dir.create(work)
  e <- new.env(parent=globalenv()); stores <- new.env(parent=emptyenv())
  messages <- new.env(parent=emptyenv()); messages$values <- list()
  termination <- new.env(parent=emptyenv())
  owner <- new.env(parent=emptyenv()); owner$state <- "build"
  e$output <- directory
  declaration <- list(index=0L, warmup=TRUE, work_directory=work,
                      output_name="out.csv", entry=file.path(work,"spec.yaml"))
  e$request <- list(repetitions=list(declaration,declaration), unavailable=list())
  e$response <- list(repetitions=list(),failure=NULL)
  e$write_json <- function(path,value) {
    if(basename(path)=="response.json" && length(value$repetitions) &&
       !is.null(value$repetitions[[1]]$failure)) stop("independent secondary response transport failure")
    assign(basename(path),value,envir=stores)
  }
  e$message <- function(...) messages$values <- c(messages$values,list(paste0(...,collapse="")))
  e$timed <- function(state,name,expr) {
    on.exit(state$durations[[name]] <- list(authored_reached_marker=TRUE))
    force(expr)
  }
  e$frame <- function(value) if(is.null(value)) NULL else list(synthetic=TRUE)
  e$issues <- function(value) list(synthetic=TRUE)
  namespace <- new.env(parent=emptyenv())
  namespace$.domain_observations <- function(result) charToRaw(paste0('{"owner":"',owner$state,'"}'))
  e$asNamespace <- function(name) namespace
  e$metadata <- function() list(synthetic=TRUE)
  e$domain <- function(entry) list(output=data.frame(ID=1L),issues=data.frame(),
                                 verification_log=NULL,warning_log=NULL,save=function() {
    owner$state <- "failed-publication"
    writeBin(charToRaw("independent completed prefix"),file.path(work,"out.csv"))
    if(interrupted) stop(structure(list(message="independent original interruption",call=NULL),
                                  class=c("interrupt","condition")))
    stop("independent original Save cause")
  })
  e$quit <- function(status) {
    termination$status <- status
    stop(structure(list(message="synthetic quit",call=NULL),class=c("syntheticQuit","error","condition")))
  }
  eval(helper[[1]],envir=e)
  tryCatch(eval(loop[[1]],envir=e),syntheticQuit=function(error) NULL)
  primary <- if(interrupted) "independent original interruption" else "independent original Save cause"
  row <- e$response$repetitions[[1]]
  stopifnot(is.null(e$result),identical(termination$status,1L),
            length(e$response$repetitions)==1L,identical(row$failure$message,primary),
            identical(e$response$failure,row$failure),
            identical(row$evidence_capture_failures[[1]]$location,"final_response"),
            any(grepl(primary,unlist(messages$values),fixed=TRUE)),
            any(grepl("independent secondary response transport failure",unlist(messages$values),fixed=TRUE)),
            !("held_save_2" %in% names(row$durations)),
            is.null(get("response.json",envir=stores)$repetitions[[1]]$failure),
            identical(rawToChar(readBin(file.path(directory,row$reports$save_1),"raw",n=10000)),
                      '{"owner":"failed-publication"}'),
            identical(rawToChar(readBin(file.path(directory,row$saved_files$save_1),"raw",n=10000)),
                      "independent completed prefix"))
}
cat("actual R final-response failure and interruption controls PASS\\n")
""",
                encoding="utf-8",
            )
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
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("interruption controls PASS", result.stdout)


if __name__ == "__main__":
    unittest.main()
