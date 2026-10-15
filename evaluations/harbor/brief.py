"""Render the brief tier from an authored full prompt.

The brief keeps the opening, column list and paths verbatim, dropping the
rules. Builders and documentation call ``brief`` directly; the CLI prints
one derived prompt without creating source files::

    python -m evaluations.harbor.brief evaluations/harbor/prompts/<case>/full.md

The tier contract is in ``prompts/README.md``.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

OPENING = "Following CDISC "
COLUMNS = "should contain the following columns in this order:"
PATHS = "Read the source datasets from /app/input"


def parts(full: str) -> tuple[str, str, list[str], str]:
    """The opening, column list, rules, and paths paragraphs of a full
    prompt, or a ValueError naming the part that is missing."""
    paragraphs = [p for p in re.split(r"\n[ \t]*\n", full.strip()) if p.strip()]
    if len(paragraphs) < 4:
        raise ValueError("want an opening, a column list, rules, and the paths")
    opening, columns, *rules, paths = paragraphs
    if not opening.startswith(OPENING):
        raise ValueError(f"the opening paragraph does not start {OPENING!r}")
    if COLUMNS not in columns:
        raise ValueError(f"the second paragraph does not say {COLUMNS!r}")
    if any(COLUMNS in rule for rule in rules):
        raise ValueError("a column list sits outside the second paragraph")
    if not paths.startswith(PATHS):
        raise ValueError(f"the last paragraph does not start {PATHS!r}")
    return opening, columns, rules, paths


def brief(full: str) -> str:
    opening, columns, _, paths = parts(full)
    return f"{opening}\n\n{columns}\n\n{paths}\n"


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Render a brief prompt without writing generated source files."
    )
    parser.add_argument("full", type=Path)
    args = parser.parse_args()
    print(brief(args.full.read_text(encoding="utf-8")), end="")


if __name__ == "__main__":
    main()
