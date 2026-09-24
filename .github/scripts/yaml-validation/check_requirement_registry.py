#!/usr/bin/env python3
"""Requirement registry check for the rules/ tree.

Every requirement ID is defined exactly once, as a bold dotted marker
``**REQ-0001.**`` in a markdown contract. Every other ``REQ-XXXX``
citation in ``rules/`` must resolve to one of those definitions. Fails
on:

- a requirement ID defined more than once;
- a citation that references an ID with no definition.

Fenced code blocks and inline code are stripped before scanning, so
examples and quoted syntax do not count as definitions or citations.
``rules/migration.yaml`` is scanned for citations only.
"""

import argparse
import re
import sys
from pathlib import Path

DEFINITION = re.compile(r"\*\*(REQ-[0-9]{4,})\.\*\*")
REFERENCE = re.compile(r"\bREQ-[0-9]{4,}\b")
FENCE_RUN = re.compile(r"```+|~~~+")
INLINE_CODE = re.compile(r"`[^`\n]*`")
FRONTMATTER = re.compile(r"\A---\n.*?\n---\n", re.DOTALL)


def strip_code(text):
    """Yield lintable prose lines: no frontmatter, fenced blocks, or code.

    A fence marker may open mid-line (e.g. ``**REQ-0160.** ```text``);
    text before the marker still counts as prose.
    """
    body = FRONTMATTER.sub("", text)
    in_fence = False
    for line in body.splitlines():
        markers = FENCE_RUN.findall(line)
        if markers:
            if not in_fence:
                head = line[: FENCE_RUN.search(line).start()]
                if head.strip():
                    yield INLINE_CODE.sub("", head)
            if len(markers) % 2 == 1:
                in_fence = not in_fence
            continue
        if in_fence:
            continue
        yield INLINE_CODE.sub("", line)


def check(root):
    errors = []
    rules = root / "rules"
    defined = {}
    cited = {}
    for path in sorted(rules.glob("**/*.md")):
        label = path.relative_to(rules).as_posix()
        body = "\n".join(strip_code(path.read_text(encoding="ascii")))
        for identifier in DEFINITION.findall(body):
            defined.setdefault(identifier, []).append(label)
        for identifier in REFERENCE.findall(body):
            cited.setdefault(identifier, set()).add(label)
    migration = rules / "migration.yaml"
    if migration.is_file():
        for identifier in REFERENCE.findall(migration.read_text(encoding="ascii")):
            cited.setdefault(identifier, set()).add("migration.yaml")
    for identifier in sorted(defined):
        locations = defined[identifier]
        if len(locations) > 1:
            errors.append(
                f"duplicate requirement definition: {identifier} "
                f"({len(locations)}x in {', '.join(sorted(set(locations)))})"
            )
    for identifier in sorted(set(cited) - set(defined)):
        files = sorted(cited[identifier])
        errors.append(
            f"unresolved requirement citation: {identifier} ({', '.join(files)})"
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
        print(f"FAIL: {len(errors)} requirement-registry violation(s).")
        return 1
    print("PASS: every requirement ID is defined once and every citation resolves.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
