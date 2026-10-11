"""Record a Windows R native-call failure without weakening qualification."""

import argparse
import os
import shutil
import subprocess
from pathlib import Path

CAPABILITIES_FIRST = """
suppressPackageStartupMessages(library(yamaa))
cat("loaded yamaa\\n"); flush.console()
print(getDLLRegisteredRoutines("yamaa")$.Call[["wrap__engine_info"]])
cat("entering string capability call\\n"); flush.console()
print(dataset_capabilities())
cat("entering list metadata call\\n"); flush.console()
print(engine_info())
cat("native calls completed\\n"); flush.console()
"""

METADATA_FIRST = """
suppressPackageStartupMessages(library(yamaa))
cat("loaded yamaa; first native call is metadata\\n"); flush.console()
writeLines(c(as.character(getRversion()), as.character(packageVersion("yamaa")),
             engine_info()$core_version, normalizePath(find.package("yamaa"))))
cat("native metadata completed\\n"); flush.console()
"""

BASE_ONLY = """
cat("entered base-only frontend control\\n"); flush.console()
writeLines(c(as.character(getRversion()), normalizePath(R.home())))
cat("base-only frontend completed\\n"); flush.console()
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, R_LIBS_USER=str(args.library.resolve()))
    executable = subprocess.check_output(
        [
            "Rscript",
            "--vanilla",
            "-e",
            'cat(normalizePath(file.path(R.home("bin"), "Rterm.exe"), winslash="/", mustWork=TRUE))',
        ],
        env=environment,
        text=True,
    ).strip()
    metadata_script = args.output.resolve() / "metadata-first.R"
    metadata_script.write_text(METADATA_FIRST, encoding="utf-8")
    base_script = args.output.resolve() / "base-only.R"
    base_script.write_text(BASE_ONLY, encoding="utf-8")
    probes = (
        ("rterm-metadata-first", [executable, "--vanilla", "--slave", "-e", METADATA_FIRST]),
        ("rscript-metadata-first", ["Rscript", "--vanilla", "-e", METADATA_FIRST]),
        ("rscript-base-only", ["Rscript", "--vanilla", "-e", BASE_ONLY]),
        ("rscript-file-metadata-first", ["Rscript", "--vanilla", str(metadata_script)]),
        ("rscript-file-base-only", ["Rscript", "--vanilla", str(base_script)]),
    )
    failed = False
    for name, command in probes:
        result = subprocess.run(
            command,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        (args.output / (name + ".txt")).write_text(result.stdout, encoding="utf-8")
        print(name, flush=True)
        print(result.stdout, flush=True)
        if not result.returncode:
            continue
        failed = True
        print(f"Native R call process exit: {result.returncode}", flush=True)
        tools = Path(environment["RTOOLS45_HOME"])
        debugger = next(tools.rglob("gdb.exe"), None)
        if debugger is not None:
            diagnostic = subprocess.run(
                [
                    str(debugger),
                    "--batch",
                    "-ex",
                    "set pagination off",
                    "-ex",
                    "run",
                    "-ex",
                    "thread apply all bt",
                    "-ex",
                    "info registers",
                    "-ex",
                    "x/12i $pc",
                    "--args",
                    *command,
                ],
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=False,
            )
            (args.output / (name + "-debugger.txt")).write_text(
                diagnostic.stdout, encoding="utf-8"
            )
            print(diagnostic.stdout, flush=True)
        else:
            print("Rtools debugger unavailable", flush=True)
    if failed:
        for dll in args.library.glob("yamaa/libs/**/*.dll"):
            shutil.copy2(dll, args.output / dll.name)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
