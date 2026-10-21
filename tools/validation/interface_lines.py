"""The line manifests of the interface stability check (#3030 P-STAB-1).

Each manifest lists frozen elements one per entry or line. `compare` diffs the
working tree's copy with the baseline release tag's
(`tools/validation/check_interface_stability.py`):

- `yaml/conditions.yaml`: each condition, with the phases that may raise it. A
  removed condition, or a phase removed from a condition, breaks. Messages are
  not compared.
- `rust/interface/manifest.txt`: a line `<element>: <facts>`, or a bare
  `<element>`, per frozen element of the bindings definition. A removed line
  breaks: the element is removed, or changed when it keeps other facts. An
  added line is an addition. Comment lines are ignored, and an element listed
  on several lines, such as `cli.synopsis`, compares as the set of its facts.
- `rust/crates/yamaa/public-api.txt`: a line per public item of the `yamaa`
  crate, each distinct line one element. A removed line breaks, so a changed
  signature shows as a removed line plus an added one.

A manifest the baseline lacks is new, so each of its elements is an addition.
One that the working tree lacks fails the check.
"""

import yaml
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.interface_manifests import Change, ManifestError

CONDITIONS = "yaml/conditions.yaml"
INTERFACE = "rust/interface/manifest.txt"
PUBLIC_API = "rust/crates/yamaa/public-api.txt"


def compare(baseline, current):
    """Every change of the line manifests from `baseline` to `current`."""
    return [
        *_compare_conditions(baseline, current),
        *_compare_interface(baseline, current),
        *_compare_public_api(baseline, current),
    ]


def _text(tree, path, required):
    text = tree.text(path)
    if text is None and required:
        raise ManifestError(f"{path}: missing from {tree}")
    return text


def _lines(text):
    """The lines of a line manifest, without blank and comment lines."""
    return [line for line in text.splitlines() if line.strip() and line[0] != "#"]


def conditions(tree, required=True):
    """Map each condition of the registry to its phases; None when absent."""
    text = _text(tree, CONDITIONS, required)
    if text is None:
        return None
    try:
        document = yaml.load(text, Loader=UniqueKeyLoader)
    except (yaml.YAMLError, ValueError) as error:
        raise ManifestError(f"{CONDITIONS} at {tree}: {error}") from None
    entries = document.get("conditions") if isinstance(document, dict) else None
    if not isinstance(entries, dict) or not all(
        isinstance(name, str)
        and isinstance(entry, dict)
        and isinstance(entry.get("phases"), list)
        and all(isinstance(phase, str) for phase in entry["phases"])
        for name, entry in entries.items()
    ):
        raise ManifestError(
            f"{CONDITIONS} at {tree}: expected `conditions`, "
            "mapping each name to its `phases`"
        )
    return {name: entry["phases"] for name, entry in entries.items()}


def _phases(phases, verb):
    noun = "phase" if len(phases) == 1 else "phases"
    return f"{noun} {', '.join(phases)} {verb}"


def _compare_conditions(baseline, current):
    before = conditions(baseline, required=False) or {}
    after = conditions(current)
    for name, phases in before.items():
        if name not in after:
            yield Change(CONDITIONS, name, "removed", breaking=True)
        elif lost := [phase for phase in phases if phase not in after[name]]:
            yield Change(CONDITIONS, name, _phases(lost, "removed"), breaking=True)
    for name, phases in after.items():
        if name not in before:
            yield Change(CONDITIONS, name, "added", breaking=False)
        elif gained := [phase for phase in phases if phase not in before[name]]:
            yield Change(CONDITIONS, name, _phases(gained, "added"), breaking=False)


def interface_elements(tree, required=True):
    """Map each element of the interface manifest to the facts of its lines,
    in order: a bare line's facts are empty. None when absent."""
    text = _text(tree, INTERFACE, required)
    if text is None:
        return None
    elements = {}
    for line in _lines(text):
        element, _, facts = line.partition(": ")
        elements.setdefault(element, []).append(facts)
    return elements


def _quoted(facts):
    return ", ".join(f"`{fact}`" for fact in facts)


def _lines_of(verb, facts):
    """Lines removed or added: the verb, then their facts, if any."""
    facts = [fact for fact in facts if fact]
    return f"{verb} {_quoted(facts)}" if facts else verb


def _compare_interface(baseline, current):
    before = interface_elements(baseline, required=False) or {}
    after = interface_elements(current)
    for element, facts in before.items():
        lost = [fact for fact in facts if fact not in after.get(element, ())]
        gained = [fact for fact in after.get(element, ()) if fact not in facts]
        if lost and gained:
            what = f"changed from {_quoted(lost)} to {_quoted(gained)}"
            yield Change(INTERFACE, element, what, breaking=True)
        elif lost:
            yield Change(INTERFACE, element, _lines_of("removed", lost), breaking=True)
    for element, facts in after.items():
        old = before.get(element, ())
        gained = [fact for fact in facts if fact not in old]
        # A changed element reported its new facts with its break.
        if gained and all(fact in facts for fact in old):
            yield Change(INTERFACE, element, _lines_of("added", gained), breaking=False)


def public_api_lines(tree, required=True):
    """The distinct lines of the public API inventory; None when absent."""
    text = _text(tree, PUBLIC_API, required)
    if text is None:
        return None
    return list(dict.fromkeys(_lines(text)))


def _compare_public_api(baseline, current):
    before = public_api_lines(baseline, required=False) or []
    after = public_api_lines(current)
    kept = set(after)
    for line in before:
        if line not in kept:
            yield Change(PUBLIC_API, line, "removed", breaking=True)
    known = set(before)
    for line in after:
        if line not in known:
            yield Change(PUBLIC_API, line, "added", breaking=False)
