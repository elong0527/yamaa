#!/usr/bin/env python3
"""Lint rules prose for rationale phrases.

Contracts state what is expected, never why. The sanctioned place for
"why" is each contract's Rationale section. This check scans every
``rules/**/*.md`` file outside fenced code blocks, inline code, and
Rationale sections, and fails on phrases that signal rationale prose:

- because
- in order to
- the reason is
- this ensures
- so that
- to ensure

Allowlist: ALLOWLIST maps a relative path (from the rules/ directory) to
a list of ``(pattern, reason)`` entries. An entry exempts a line matching
the regex ``pattern``; every entry documents why the occurrence is
genuinely unavoidable. Prefer rewording over allowlisting.
"""

import argparse
import re
import sys
from pathlib import Path

PHRASES = [
    r"because",
    r"in order to",
    r"the reason is",
    r"this ensures",
    r"so that",
    r"to ensure",
]
PATTERN = re.compile(r"\b(?:" + "|".join(PHRASES) + r")\b", re.IGNORECASE)
FENCE_RUN = re.compile(r"```+|~~~+")
INLINE_CODE = re.compile(r"`[^`\n]*`")
RATIONALE_HEADING = re.compile(r"^##\s+Rationale\s*$")
ANY_HEADING = re.compile(r"^##\s+")
FRONTMATTER = re.compile(r"\A---\n.*?\n---\n", re.DOTALL)

# (relative path from rules/, line pattern, why it cannot be reworded)
ALLOWLIST = {}


def strip_prose(path):
    """Yield (line number, text) for lintable prose lines of a markdown file.

    Drops frontmatter, fenced code blocks, and inline code; text before a
    mid-line fence marker still counts as prose. Rationale sections are
    skipped: "why" belongs there.
    """
    body = FRONTMATTER.sub("", path.read_text(encoding="ascii"))
    in_fence = False
    in_rationale = False
    for number, raw in enumerate(body.splitlines(), start=1):
        markers = FENCE_RUN.findall(raw)
        if markers:
            # A fence marker may open mid-line after prose (e.g.
            # "**REQ-0160.** ```text"); the prose head still counts.
            head = raw[: FENCE_RUN.search(raw).start()]
            if not in_fence and head.strip():
                line = INLINE_CODE.sub("", head)
                if RATIONALE_HEADING.match(line):
                    in_rationale = True
                elif ANY_HEADING.match(line):
                    in_rationale = False
                if not in_rationale:
                    yield number, line
            if len(markers) % 2 == 1:
                in_fence = not in_fence
            continue
        if in_fence:
            continue
        line = raw
        if RATIONALE_HEADING.match(line):
            in_rationale = True
            continue
        if ANY_HEADING.match(line):
            in_rationale = False
        if in_rationale:
            continue
        yield number, INLINE_CODE.sub("", line)


def check(root):
    errors = []
    rules = root / "rules"
    for path in sorted(rules.glob("**/*.md")):
        label = path.relative_to(rules).as_posix()
        allowed = ALLOWLIST.get(label, [])
        for number, line in strip_prose(path):
            match = PATTERN.search(line)
            if not match:
                continue
            if any(re.search(pattern, line) for pattern, _ in allowed):
                continue
            errors.append(
                f"{label}:{number}: rationale phrase '{match.group(0)}': {line.strip()}"
            )
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[3],
        help="Repository root directory",
    )
    args = parser.parse_args()
    errors = check(args.root)
    for error in errors:
        print(error)
    if errors:
        print(f"FAIL: {len(errors)} rationale-phrase violation(s).")
        return 1
    print("PASS: rules prose has no rationale phrases outside Rationale sections.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
