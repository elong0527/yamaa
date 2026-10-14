"""Prove the plain R child's PID, touched allocation and original nonzero exit.

This is an authored process control before collection, not a timing sample or
memory budget. Preserve its script, stdout/stderr and kernel observation.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

import child_peak_rss

ALLOCATION = 64 * 1024 * 1024
PAGE_STRIDE = 4096
EXIT_CODE = 7
CONTROL = r"""
args <- commandArgs(TRUE)
library(jsonlite)
root <- normalizePath(Sys.getenv("R_LIBS"), winslash="/", mustWork=TRUE)
stopifnot(as.character(getRversion()) == "4.6.1")
stopifnot(as.character(packageVersion("jsonlite")) == "2.0.0")
stopifnot(dirname(normalizePath(find.package("jsonlite"), winslash="/")) == root)
buffer <- raw(64L * 1024L * 1024L)
pages <- seq.int(1L, length(buffer), by=4096L)
buffer[pages] <- as.raw(165L)
stopifnot(length(buffer) == 64L * 1024L * 1024L)
stopifnot(all(buffer[pages] == as.raw(165L)))
jsonlite::write_json(list(
  pid=Sys.getpid(), runtime_version=as.character(getRversion()),
  private_library=root, jsonlite_version=as.character(packageVersion("jsonlite")),
  allocation_bytes=length(buffer), page_stride=4096L,
  touched_page_count=length(pages), touched_byte_sum=sum(as.integer(buffer[pages])),
  requested_exit_code=7L), args[1], auto_unbox=TRUE, pretty=TRUE)
quit(save="no", status=7L, runLast=FALSE)
"""


def validate(process, response, system, private_library):
    if (
        type(response["pid"]) is not int
        or response["pid"] <= 0
        or type(process["pid"]) is not int
        or process["pid"] != response["pid"]
        or type(process["exit_code"]) is not int
        or process["exit_code"] != EXIT_CODE
        or response["requested_exit_code"] != EXIT_CODE
        or response["runtime_version"] != "4.6.1"
        or response["jsonlite_version"] != "2.0.0"
        or not isinstance(response["private_library"], str)
        or os.path.normcase(os.path.abspath(response["private_library"]))
        != os.path.normcase(os.path.abspath(private_library))
        or response["allocation_bytes"] != ALLOCATION
        or response["page_stride"] != PAGE_STRIDE
        or response["touched_page_count"] != ALLOCATION // PAGE_STRIDE
        or response["touched_byte_sum"] != 165 * (ALLOCATION // PAGE_STRIDE)
    ):
        raise ValueError("plain R PID, allocation, private namespace or exit differs")
    child_peak_rss.validate(process["peak_rss"], system)
    if process["peak_rss"]["bytes"] < ALLOCATION:
        raise ValueError("plain R child peak excludes its touched allocation")


def check(args):
    args.output.mkdir(parents=True, exist_ok=False)
    script = args.output / "control.R"
    script.write_text(CONTROL, encoding="utf-8")
    shutil.copyfile(__file__, args.output / Path(__file__).name)
    response_path = args.output / "r-process.json"
    process_path = args.output / "process.json"
    command = [str(args.rscript), "--vanilla", str(script), str(response_path)]
    broker = [
        sys.executable,
        "-I",
        str(Path(child_peak_rss.__file__)),
        "--command",
        json.dumps(command),
        "--record",
        str(process_path),
    ]
    # R is deliberately a plain launch: never apply Python venv redirectors.
    record = {
        "scope": "authored plain-R PID/RSS/exit control; no timing samples",
        "system": platform.system(),
        "command": command,
        "broker_command": broker,
        "private_library": os.environ["R_LIBS"],
        "accepted": False,
    }
    destination = args.output / "r-child-rss-control.json"
    destination.write_text(json.dumps(record, indent=2) + "\n")
    try:
        with (
            (args.output / "stdout.txt").open("xb") as stdout,
            (args.output / "stderr.txt").open("xb") as stderr,
        ):
            subprocess.run(broker, stdout=stdout, stderr=stderr, check=True)
        process = json.loads(process_path.read_text())
        response = json.loads(response_path.read_text())
        private = Path(os.environ["R_LIBS"]).resolve().as_posix()
        validate(process, response, platform.system(), private)
        record.update(
            accepted=True,
            observed_pid=response["pid"],
            observed_exit_code=process["exit_code"],
            observed_peak_rss_bytes=process["peak_rss"]["bytes"],
        )
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        record["failure"] = {"class": type(error).__name__, "reason": str(error)}
        raise
    finally:
        destination.write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rscript", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    check(parser.parse_args())
