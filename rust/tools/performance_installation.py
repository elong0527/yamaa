"""Record actual installed origins and explicit source/build association for CI."""

import argparse
import json
import platform
import subprocess
import sys
from pathlib import Path

import measure_public_lifecycle as lifecycle

PYTHON_INFO = """
import importlib.util,json,pathlib,platform,yamaa
print(json.dumps({'runtime_version':platform.python_version(),'package_version':yamaa.__version__,
 'installed_origins':{'package':str(pathlib.Path(yamaa.__file__).resolve().parent),
 'native':str(pathlib.Path(importlib.util.find_spec('yamaa._native').origin).resolve())}}))
"""

R_INFO = """
library(yamaa)
library(microbenchmark)
collectors <- lapply(c('jsonlite','microbenchmark'),function(name) list(
 version=as.character(utils::packageVersion(name)),
 origin=normalizePath(find.package(name),winslash='/',mustWork=TRUE)))
names(collectors) <- c('jsonlite','microbenchmark')
cat(jsonlite::toJSON(list(runtime_version=as.character(getRversion()),
 package_version=as.character(utils::packageVersion('yamaa')),
 installed_origins=list(package=normalizePath(find.package('yamaa'),winslash='/',mustWork=TRUE),
 native=normalizePath(getLoadedDLLs()[['yamaa']][['path']],winslash='/',mustWork=TRUE)),
 collectors=collectors,
 clock_library=normalizePath(getLoadedDLLs()[['microbenchmark']][['path']],winslash='/',mustWork=TRUE)),
 auto_unbox=TRUE))
"""


def record(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    script = output / ("inspect.py" if args.host == "python" else "inspect.R")
    script.write_text(PYTHON_INFO if args.host == "python" else R_INFO)
    command = ([str(args.executable), "-I", str(script)] if args.host == "python"
               else [str(args.executable), "--vanilla", str(script)])
    observation = subprocess.run(command, capture_output=True, text=True, check=True)
    (output / "inspection-stdout.txt").write_text(observation.stdout)
    (output / "inspection-stderr.txt").write_text(observation.stderr)
    info = json.loads(observation.stdout)
    original = lifecycle.git_bytes(args.repository, args.revision, "rust/Cargo.toml").decode()
    import tomllib
    core_version = tomllib.loads(original)["workspace"]["package"]["version"]
    result = dict(info, host=args.host, source_revision=args.revision,
        artifact_reference=args.artifact_reference, artifact_path=str(args.artifact.resolve()),
        package_form=args.form, engine_version=core_version,
        system=platform.system(), cpu=platform.processor() or None,
        toolchain={"rust": "1.90.0", "cargo": "1.90.0",
                   "association": args.build_association},
        installation_evidence=str(output / "installation.json"))
    if args.host == "r":
        if not args.clock_config or not args.clock_library:
            raise ValueError("retained timer build config/library required")
        config = args.clock_config.read_text(errors="replace")
        if platform.system() == "Windows":
            provider = "QueryPerformanceCounter"
        elif "#define MB_HAVE_MACH_TIME 1" in config:
            provider = "mach_absolute_time"
        else:
            definitions = {line.split()[1]:line.split()[2] for line in config.splitlines()
                           if len(line.split()) >= 3 and line.split()[0] == "#define"}
            provider = "clock_gettime/" + definitions.get("MB_CLOCKID_T", "unavailable")
        result["clock_evidence"] = {"provider":provider,
            "build_config":str(args.clock_config.resolve()),
            "built_library":str(args.clock_library.resolve()),
            "installed_library":result.pop("clock_library")}
    installation = {"evidence":{key:result[key] for key in
        ("source_revision","artifact_reference","package_form")},
        "installation":{"runtime_version":info["runtime_version"],
            "host_package_version":info["package_version"], "core_version":core_version,
            "package_location":info["installed_origins"]["native" if args.host == "python" else "package"]},
        "build_association":args.build_association}
    lifecycle.write_json(output / "installation.json", installation)
    lifecycle.write_json(output / "provenance.json", result)
    return result


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository",type=Path,required=True)
    parser.add_argument("--revision",required=True)
    parser.add_argument("--host",choices=("python","r"),required=True)
    parser.add_argument("--executable",type=Path,required=True)
    parser.add_argument("--artifact",type=Path,required=True)
    parser.add_argument("--artifact-reference",required=True)
    parser.add_argument("--form",choices=("wheel","source"),required=True)
    parser.add_argument("--build-association",required=True)
    parser.add_argument("--clock-config",type=Path)
    parser.add_argument("--clock-library",type=Path)
    parser.add_argument("--output",type=Path,required=True)
    record(parser.parse_args())
