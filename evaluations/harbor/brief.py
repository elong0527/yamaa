"""Write each benchmark's brief prompt from its full prompt.

A full prompt, `prompts/<benchmark>/full.md`, has four parts, in order:

    Following CDISC ... standards, ...          the opening paragraph
    The output dataset should contain ...       the column list
    <one or more paragraphs>                    the rules
    Read the source datasets from /app/input    the input and output paths

The brief prompt, `prompts/<benchmark>/brief.md`, is the full prompt without
its rules: the opening, the column list, and the paths, verbatim. What each
tier means is in `prompts/README.md`. Run from the repository root after
changing a full prompt:

    uv run --project python --no-sync python evaluations/harbor/brief.py
"""

from __future__ import annotations

import re
from pathlib import Path

PROMPTS = Path(__file__).resolve().parent / "prompts"
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
    for full in sorted(PROMPTS.glob("*/full.md")):
        text = brief(full.read_text(encoding="utf-8"))
        (full.parent / "brief.md").write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
