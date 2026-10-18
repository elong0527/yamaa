"""Check a written prompt against its sources, never its expected data.

`packet.py` renders `check.json` beside the writer's sources: the parts of
the request the benchmark fixes (`scaffold.py`), the names a prompt never
says, the README, and what a trace may cite. This file needs only the
standard library and `brief.py`, so the same checks run in the writer's
sandbox, in the writing task's verifier, and in the repository tests::

    python3 check.py full full.md --data check.json [--trace trace.json]
    python3 check.py conventions conventions.md --full full.md

With `--reward` it writes Harbor's `reward.json` and exits 0 either way: a
prompt that fails its checks scores 0 rather than erroring the verifier.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

try:
    from evaluations.harbor import brief
except ImportError:  # beside brief.py in a writing task
    import brief

# What a prompt never mentions (prompts/recipe.md).
UNSAID = re.compile(
    r"\b(?:yamaa|yaml|spec|specifications?|schemas?|handlers?|verifications?"
    r"|derivations?)\b|\bR0\d\d\b|\bREQ-\d+",
    re.IGNORECASE,
)
# Wording that tells the agent how to compute rather than what a value is:
# joins, filters, sorting, keeping a record, loops, steps and code. Literal
# values are set aside first (`_prose`), so a "FILTER" label is not wording,
# and a join that names both inputs, `Join AE to DM on USUBJID`, reads
# `Join to on`.
PROCEDURE = re.compile(
    r"\b(?:left|right|inner|outer|full|cross|anti|semi)[ -]join\b"
    r"|\bjoin(?:ed|ing|s)?(?: (?:to|with|and|onto|into))? on\b"
    r"|\bmerg(?:e|ed|es|ing)\b|\bfilter(?:ed|ing|s)?\b"
    r"|\bgroup(?:ed|ing|s)? by\b"
    r"|\bsort(?:ed|ing|s)? (?:the )?(?:records|rows|data(?:sets?)?|by)\b"
    r"|\b(?:keep|keeps|kept|retain|retains|retained) (?:only )?the (?:first|last)\b"
    r"|\bfirst[ _]available\b|\b(?:loop|iterate)s? (?:over|through)\b"
    r"|\bfor each row\b|\bdata ?frames?\b|\bpseudo-?code\b"
    r"|\b(?:step \d+|first step|next step|final step)\b"
    r"|\bde-?duplicat\w*|\b(?:drop|remove) duplicates\b|\bcoalesce\w*"
    r"|\bintermediate (?:datasets?|results?|variables?|columns?|tables?)\b"
    r"|==|!=|<-|:=",
    re.IGNORECASE,
)
# The verbs of those procedures, as whole words. One written in capitals
# ("FILTER AE to fatal records") is still wording, so `_prose` keeps it; a
# code that only starts like one (SORTORD, COALESCEVAL) is a code.
VERBS = re.compile(
    r"join(?:ed|ing|s)?|merg(?:e|ed|es|ing)|filter(?:ed|ing|s)?"
    r"|group(?:ed|ing|s)?|sort(?:ed|ing|s)?|keeps?|kept|retain(?:ed|s)?"
    r"|(?:loop|iterate)s?|coalesce[ds]?|deduplicat(?:e|ed|es|ing)|drop|remove",
    re.IGNORECASE,
)
# What a conventions prompt may name: a quoted value, an upper-case code or
# variable, or a number. Each must come from the full prompt.
LITERALS = re.compile(r'"[^"]*"|\b[A-Z][A-Z0-9_]+\b|\b\d+(?:\.\d+)?\b')
# A skeleton placeholder (`scaffold.py`), possibly wrapped across lines.
PLACEHOLDER = re.compile(r"<[a-z]+(?:\s+[a-z]+)*>")
WIDTH = 79
CONVENTIONS_LINES = 20
# A trace source: `README: "<quote>"`, or a file and what it holds.
SOURCE = re.compile(r"(?P<file>[^:]+):\s*(?P<ref>.+)", re.DOTALL)


def words(text: str) -> str:
    return " ".join(text.split())


def _prose(text: str) -> str:
    """The text without its literal values. An upper-case code leaves
    nothing, so the words around it meet; a quoted value leaves `""`, so
    the words on either side never read as one phrase: `joined with "T" on`
    is not `joined with on`. A procedure verb in capitals (`VERBS`) is
    wording and stays."""

    def literal(match: re.Match[str]) -> str:
        value = match.group(0)
        if value.startswith('"'):
            return '""'
        return value if VERBS.fullmatch(value) else " "

    return re.sub(r'"[^"]*"|\b[A-Z][A-Z0-9_]+\b', literal, text)


def procedure_problems(text: str) -> list[str]:
    """Steps, joins, filters, loops or code, which a prompt never gives."""
    return [
        f"reads as a procedure: {match.group(0)!r}"
        for match in PROCEDURE.finditer(words(_prose(text)))
    ]


def recipe_problems(text: str) -> list[str]:
    """What every tier keeps to: no harness names or procedure, and lines
    within 79 columns."""
    problems = [f"names the harness: {m.group(0)!r}" for m in UNSAID.finditer(text)]
    problems += procedure_problems(text)
    problems += [
        f"line {number} is wider than {WIDTH} columns"
        for number, line in enumerate(text.splitlines(), 1)
        if len(line) > WIDTH
    ]
    return problems


def unsaid_problems(text: str, unsaid: dict[str, str]) -> list[str]:
    """Fixture identifiers and dates, and names only the specification uses."""
    return [
        f"says {name!r}, {reason}"
        for name, reason in sorted(unsaid.items())
        if re.search(rf"(?<![\w-]){re.escape(name)}(?![\w-])", text)
    ]


def full_problems(text: str, data: dict) -> list[str]:
    """A full prompt against the parts its benchmark fixes."""
    try:
        opening, columns, _, paths = brief.parts(text)
    except ValueError as exc:
        return [str(exc)]
    problems = []
    if not opening.startswith(data["opening"]):
        problems.append(f"the opening does not start {data['opening']!r}")
    problems += [
        f"the opening does not name {domain}"
        for domain in data["domains"]
        if not re.search(rf"\b{re.escape(domain)}\b", opening)
    ]
    if words(columns) != words(data["columns"]):
        problems.append("the column lists are not the ones the outputs fix")
    if words(paths) != words(data["paths"]):
        problems.append("the paths are not the ones the outputs fix")
    if PLACEHOLDER.search(text):
        problems.append("a skeleton placeholder is left")
    return problems + recipe_problems(text) + unsaid_problems(text, data["unsaid"])


def sentences(rules: list[str]) -> list[str]:
    """Each sentence of the rules, a list item apart from its lead-in."""
    found = []
    for paragraph in rules:
        items: list[list[str]] = []
        for line in paragraph.splitlines():
            item = line.strip()
            if item.startswith("- ") or not items:
                items.append([item.removeprefix("- ")])
            else:
                items[-1].append(item)
        for item in items:
            found += re.split(r"(?<=[.!?])\s+(?=[A-Z0-9\"'(])", words(" ".join(item)))
    return [s for s in found if s]


def trace_problems(text: str, trace: object, data: dict) -> list[str]:
    """Every sentence of the rules quoted in the trace, with real sources.

    A source is `README: "<quote>"`, a specification file and a dotted path
    (`spec.yaml: columns.DTHDT.derivation`), an input file and the column or
    value it holds, or a language rule file.
    """
    try:
        _, _, rules, _ = brief.parts(text)
    except ValueError as exc:
        return [str(exc)]
    if not isinstance(trace, list) or not trace:
        return ["the trace has no entries"]
    said, readme = words(" ".join(rules)), words(data["readme"])
    problems, quotes = [], []
    for number, entry in enumerate(trace, 1):
        entry = entry if isinstance(entry, dict) else {}
        quote = words(str(entry.get("prompt", "")))
        if not quote or quote not in said:
            problems.append(f"trace entry {number} quotes text the rules do not hold")
        quotes.append(quote)
        sources = entry.get("sources")
        if not isinstance(sources, list) or not sources:
            problems.append(f"trace entry {number} names no source")
            continue
        for source in sources:
            match = SOURCE.fullmatch(str(source).strip())
            name, ref = (
                (match["file"].strip(), words(match["ref"])) if match else ("", "")
            )
            if name == "README":
                cited = ref.strip('"')
                known = bool(cited) and cited in readme
            elif name in data["specification"]:
                known = ref in data["specification"][name]
            else:
                known = name in data["inputs"] or name in data["rules"]
            if not known:
                problems.append(
                    f"trace entry {number} cites {source!r}, which no source holds"
                )
    problems += [
        f"no trace entry covers {sentence[:60]!r}"
        for sentence in sentences(rules)
        if quotes and not any(q and (q in sentence or sentence in q) for q in quotes)
    ]
    return problems


def conventions_problems(full: str, text: str) -> list[str]:
    """A conventions prompt only narrows its full prompt (prompts/README.md)."""
    opening, columns, rules, paths = brief.parts(full)
    paragraphs = [p for p in re.split(r"\n[ \t]*\n", text.strip()) if p.strip()]
    problems = []
    if paragraphs[:2] != [opening, columns]:
        problems.append("the opening and column lists are not the full prompt's")
    if paragraphs[-1:] != [paths]:
        problems.append("the paths are not the full prompt's")
    if text != "\n\n".join(paragraphs) + "\n":
        problems.append("paragraphs are not separated by one blank line")
    if len(text.splitlines()) > CONVENTIONS_LINES:
        problems.append(f"longer than {CONVENTIONS_LINES} lines")
    kept = words(" ".join(paragraphs[2:-1]))
    if len(kept) > len(words(" ".join(rules))):
        problems.append("longer than the full prompt's rules")
    named = set(LITERALS.findall(words(full)))
    extra = sorted(set(LITERALS.findall(kept)) - named)
    if extra:
        problems.append(f"names what the full prompt does not: {', '.join(extra)}")
    return problems + recipe_problems(text)


def _read(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    tiers = parser.add_subparsers(dest="tier", required=True)
    full = tiers.add_parser("full")
    full.add_argument("--data", type=Path, required=True)
    full.add_argument("--trace", type=Path)
    conventions = tiers.add_parser("conventions")
    conventions.add_argument("--full", type=Path, required=True)
    for tier in (full, conventions):
        tier.add_argument("prompt", type=Path)
        tier.add_argument("--reward", type=Path, help="write Harbor's reward.json")
    args = parser.parse_args()
    text = _read(args.prompt)
    if text is None:
        problems = [f"{args.prompt} was not written as UTF-8 text"]
    elif args.tier == "full":
        data = json.loads(args.data.read_text(encoding="utf-8"))
        problems = full_problems(text, data)
        if args.trace is not None:
            raw = _read(args.trace)
            try:
                trace = json.loads(raw) if raw is not None else None
            except json.JSONDecodeError:
                trace = None
            problems += trace_problems(text, trace, data)
    else:
        problems = conventions_problems(args.full.read_text(encoding="utf-8"), text)
    for problem in problems:
        print(problem)
    print("PASS" if not problems else f"FAIL: {len(problems)} problem(s)")
    if args.reward is not None:
        args.reward.parent.mkdir(parents=True, exist_ok=True)
        args.reward.write_text(json.dumps({"reward": float(not problems)}) + "\n")
    elif problems:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
