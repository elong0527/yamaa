"""Copy original study projects and independent fixed-byte Define witnesses."""

import argparse
import json
import posixpath
import shutil
from pathlib import Path

import yaml
from tools.benchmark import directories, load_metadata
from tools.paths import ROOT

CASES = (
    "sdtm-dm-metadata",
    "sdtm-ae-coding",
    "sdtm-vs-epoch-from-subject-elements",
    "sdtm-lb-metadata",
)
PUBLISHED = (".csv", ".parquet")


def stage_project(case: Path, target: Path, written: str):
    """Copy the project file `written` of `case` into `target`, with each
    parent project file of its chain and the codelist files each lists, all
    at their paths from the case (PROJECT-4, PROJECT-16)."""

    def copy(relative: str):
        (target / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(case / relative, target / relative)

    seen = set()
    while written not in seen and (case / written).is_file():
        seen.add(written)
        copy(written)
        layer = yaml.safe_load((case / written).read_text(encoding="utf-8"))
        base = posixpath.dirname(written)
        for source in layer.get("codelists") or []:
            if isinstance(source, str):
                copy(posixpath.normpath(posixpath.join(base, source)))
        parent = layer.get("parent")
        if not isinstance(parent, str):
            break
        written = posixpath.normpath(posixpath.join(base, parent))


def stage_benchmarks(destination: Path):
    """Stage every benchmark declaring a Define call, with a JSON manifest.

    Each case keeps its specifications, its project file, each parent
    project file and the codelist files each lists, at their paths from the
    case, and its independent ``expected/`` truth; the published tables are
    the expected ones. The manifest carries the call and the expected error
    so hosts read no YAML.
    """
    destination.mkdir()
    cases = []
    for case in directories(ROOT / "benchmarks"):
        define = load_metadata(case).get("define")
        if define is None:
            continue
        target = destination / case.name
        target.mkdir()
        for spec in sorted(case.glob("spec*.yaml")):
            shutil.copy2(spec, target / spec.name)
        stage_project(case, target, define["environment"])
        shutil.copytree(case / "expected", target / "expected")
        error = case / "expected" / "error.yaml"
        expected = sorted(path.name for path in (case / "expected").iterdir())
        cases.append(
            {
                "name": case.name,
                "project": define["environment"],
                "standard": define["standard"],
                "created": define["created"],
                "published": [name for name in expected if name.endswith(PUBLISHED)],
                "expected": [
                    name
                    for name in expected
                    if not name.endswith(PUBLISHED) and name != "error.yaml"
                ],
                "error": (
                    yaml.safe_load(error.read_text(encoding="utf-8"))
                    if error.is_file()
                    else None
                ),
            }
        )
    (destination / "manifest.json").write_text(
        json.dumps({"cases": cases}, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )


def stage(destination: Path):
    destination.mkdir()
    # The installed Define tests validate against the vendored CDISC schemas;
    # they ship with the test fixtures since the reference submission package
    # is not part of the current API.
    shutil.copytree(ROOT / "rust/tests/schemas", destination / "schemas")
    for name in CASES:
        original = ROOT / "benchmarks" / name
        target = destination / name
        target.mkdir()
        shutil.copy2(original / "spec.yaml", target / "spec.yaml")
        if name == "sdtm-lb-metadata":
            shutil.copy2(
                original / "submission-spec.yaml", target / "submission-spec.yaml"
            )
            shutil.copy2(original / "submission-project.yaml", target / "project.yaml")
            (target / "expected").mkdir()
            shutil.copy2(original / "expected/lb.csv", target / "expected/lb.csv")
        else:
            shutil.copy2(original / "project.yaml", target / "project.yaml")
        if name == "sdtm-dm-metadata":
            expected = target / "expected"
            expected.mkdir()
            for file in ("dm.csv", "dm.json", "define.xml"):
                shutil.copy2(original / "expected" / file, expected / file)
    stage_benchmarks(destination / "benchmarks")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
