"""Tool-private issue transport from the real installed R public Check facade."""

import json
import shutil
import subprocess
from pathlib import Path
from tempfile import TemporaryDirectory

CHECK_PROGRAM = r"""
args <- commandArgs(trailingOnly = TRUE)
result <- yamaa::yamaa_check(args[[1]], environment = args[[2]])
issues <- result$issues
# Copy the issue frame; the Check facade remains read-only. AsIs preserves an
# array even when a diagnostic has exactly one path or no paths.
issues$spec_paths <- lapply(issues$spec_paths, function(paths) I(as.character(paths)))
encoded <- jsonlite::toJSON(issues, dataframe = "rows", auto_unbox = TRUE, na = "null")
stream <- file(args[[3]], open = "wb")
writeLines(enc2utf8(encoded), stream, useBytes = TRUE)
close(stream)
"""

METADATA_PROGRAM = r"""
if (!requireNamespace("yamaa", quietly = TRUE) ||
    !requireNamespace("jsonlite", quietly = TRUE)) quit(status = 9L)
args <- commandArgs(trailingOnly = TRUE)
metadata <- list(package = "yamaa", version = as.character(utils::packageVersion("yamaa")),
  core = yamaa:::.core_version(), package_path = normalizePath(find.package("yamaa")),
  fixture_path = system.file("project-original", package = "yamaa"),
  function_package_version = tryCatch(
    as.character(utils::packageVersion("yamaabenchmarks")), error = function(error) NULL))
jsonlite::write_json(metadata, args[[1]], auto_unbox = TRUE)
"""


def installation(*, required=False):
    """Read real R/SDK/native identity without Python binding or language override."""
    runner = shutil.which("Rscript")
    if runner is None:
        if required:
            raise RuntimeError("mandatory R issue transport requires Rscript")
        return None
    with TemporaryDirectory(prefix="yamaa-r-check-metadata-") as temporary:
        root = Path(temporary)
        script, output = root / "metadata.R", root / "metadata.json"
        script.write_text(METADATA_PROGRAM, encoding="utf-8")
        completed = subprocess.run(
            [runner, "--vanilla", str(script), str(output)],
            capture_output=True,
            check=False,
        )
        if completed.returncode == 9 and not required:
            return None
        if completed.returncode != 0:
            raise RuntimeError(
                "installed R metadata failed: "
                + completed.stderr.decode("utf-8", errors="replace")
            )
        metadata = json.loads(output.read_text(encoding="utf-8"))
    if metadata["package"] != "yamaa" or not metadata["core"]:
        raise RuntimeError(
            "R issue transport did not identify the native installed package"
        )
    return metadata


def rows(specification, environment, *, required=False):
    if installation(required=required) is None:
        return None
    with TemporaryDirectory(prefix="yamaa-r-check-transport-") as temporary:
        root = Path(temporary)
        script, output = root / "check.R", root / "issues.json"
        script.write_text(CHECK_PROGRAM, encoding="utf-8")
        subprocess.run(
            [
                shutil.which("Rscript"),
                "--vanilla",
                str(script),
                str(specification),
                str(environment),
                str(output),
            ],
            capture_output=True,
            check=True,
        )
        values = json.loads(output.read_text(encoding="utf-8"))
    result = []
    for value in values:
        if set(value) != {"phase", "condition", "requirement", "spec_paths", "context"}:
            raise ValueError("R issue transport narrowed the complete diagnostic row")
        if not isinstance(value["spec_paths"], list) or not all(
            isinstance(path, str) for path in value["spec_paths"]
        ):
            raise ValueError("R issue transport did not preserve array paths")
        if not isinstance(value["context"], str) or not isinstance(
            json.loads(value["context"]), dict
        ):
            raise TypeError("R issue transport did not preserve JSON-text context")
        result.append(
            (
                value["phase"],
                value["condition"],
                value["requirement"],
                value["spec_paths"],
                value["context"],
            )
        )
    return result
