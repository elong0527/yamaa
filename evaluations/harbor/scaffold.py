"""Render the parts of a full prompt that the benchmark itself fixes.

A full prompt has four parts (`prompts/README.md`): an opening, the column
lists, the rules, and the paths. All but the rules and the record level
follow from `benchmark.yaml`, the specifications that write datasets, and
the input files, so this module renders them without reading `expected/`.
The writing flow (`author.py`) hands the skeleton to the prompt's writer,
and the prompt checks (`check.py`) pin every full prompt to these parts::

    python -m evaluations.harbor.scaffold benchmarks/<case>
"""

from __future__ import annotations

import argparse
import textwrap
from pathlib import Path

import yaml
from tools.benchmark import load_metadata

WIDTH = 79
RECORD_LEVEL = "<record level>"
RULES = "<rules>"
# An initialism is read letter by letter, so its article follows the name of
# its first letter: an ADSL, an SV, a DM. A longer name is read as a word.
VOWEL_LETTERS = frozenset("AEFHILMNORSX")


def output_specs(benchmark: Path) -> list[dict]:
    """Each specification that writes a dataset, once per output file.

    `spec.yaml`, or else the `spec_<name>.yaml` files, in the order the
    benchmark's Python entries call them (producers first), then file order.
    Variants that write the same file count once.
    """
    single = benchmark / "spec.yaml"
    paths = [single] if single.is_file() else sorted(benchmark.glob("spec_*.yaml"))
    entries = load_metadata(benchmark).get("entries", {}).get("python", [])
    called = [e["specification"] for e in entries]
    paths.sort(key=lambda p: called.index(p.name) if p.name in called else len(called))
    specs, seen = [], set()
    for path in paths:
        spec = yaml.safe_load(path.read_text(encoding="utf-8"))
        name = Path((spec.get("output") or {}).get("path") or "").name
        if name and name not in seen:
            seen.add(name)
            specs.append(spec)
    return specs


def _article(domain: str) -> str:
    initialism = domain.startswith("AD") or len(domain) <= 2
    vowels = VOWEL_LETTERS if initialism else frozenset("AEIOU")
    return "an" if domain[:1] in vowels else "a"


def _series(names: list[str]) -> str:
    if len(names) < 3:
        return " and ".join(names)
    return f"{', '.join(names[:-1])}, and {names[-1]}"


def _fill(text: str) -> str:
    return textwrap.fill(text, WIDTH, break_long_words=False, break_on_hyphens=False)


def input_names(benchmark: Path) -> list[str]:
    """The datasets the agent gets, by file name: `/app/input/ae.csv` is AE.

    Input schemas (`*.schema.yaml`) never reach the agent (`build.py`)."""
    return sorted(
        p.name.partition(".")[0].upper()
        for p in (benchmark / "input").iterdir()
        if p.is_file() and p.suffix not in (".yaml", ".yml")
    )


def opening_start(benchmark: Path) -> str:
    """The words every opening starts with."""
    standard = load_metadata(benchmark)["standard"]
    return f"Following CDISC {standard} standards, use the provided "


def domains(benchmark: Path) -> list[str]:
    return [spec["domain"] for spec in output_specs(benchmark)]


def opening(benchmark: Path) -> str:
    """The opening, with the record level of each dataset left to write."""
    names = input_names(benchmark)
    noun = "dataset" if len(names) == 1 else "datasets"
    targets = " and ".join(
        f"{_article(d)} {d} dataset with one record per {RECORD_LEVEL}"
        for d in domains(benchmark)
    )
    return _fill(
        f"{opening_start(benchmark)}{_series(names)} {noun} to create {targets}."
    )


def columns_paragraph(benchmark: Path) -> str:
    """Every output column in order, one list per dataset."""
    specs = output_specs(benchmark)
    lists = []
    for spec in specs:
        subject = "output" if len(specs) == 1 else spec["domain"]
        lists.append(
            f"The {subject} dataset should contain the following columns in this "
            f"order:\n{_fill(', '.join(spec['output']['columns']))}"
        )
    return "\n".join(lists)


def paths_paragraph(benchmark: Path) -> str:
    """Where the inputs are and where each dataset is saved."""
    specs = output_specs(benchmark)
    files = [Path(spec["output"]["path"]).name for spec in specs]
    if len(specs) == 1:
        saved = f"the completed dataset as /app/output/{files[0]}"
    else:
        saved = _series(
            [
                f"the completed {spec['domain']} dataset as /app/output/{name}"
                for spec, name in zip(specs, files, strict=True)
            ]
        )
    return _fill(f"Read the source datasets from /app/input and save {saved}.")


def skeleton(benchmark: Path) -> str:
    """The full prompt with its record level and rules left to write."""
    parts = (
        opening(benchmark),
        columns_paragraph(benchmark),
        RULES,
        paths_paragraph(benchmark),
    )
    return "\n\n".join(parts) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("benchmark", type=Path)
    print(skeleton(parser.parse_args().benchmark), end="")


if __name__ == "__main__":
    main()
