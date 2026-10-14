"""R022 fixture checks through the installed native regex owner."""

from pathlib import Path, PurePosixPath

import _native_regex
from editorial import UniqueKeyLoader
from validation_diagnostics import validation_diagnostic

import yaml

# R022 pattern decisions and matches come from installed native specifications.
# The generated source belongs to each fixture and builds are never saved.
REGEX_CONTRACT = "regex"
REGEX_CONTRACT_VERSION = "2.0.0"

# The portable regex binding is imported lazily on first use. Importing the
# yamaa package eagerly pulls in its runtime dependencies (polars), which
# repository validation only needs when it actually compiles a pattern;
# keeping the import lazy drops this module's import cost from ~90MB to
# ~20MB for consumers that never touch regular expressions.
# _portable_binding is None until the first require_regex_binding() call
# attempts the import, then True (loaded) or False (unavailable).
_portable_binding = None
_PortableRegexError = None
_portable_group_count = None
_portable_compile = None
_portable_full_match = None


class RegexBindingUnavailable(Exception):
    """R022 unsupported_regex_engine: the portable binding is not installed."""


class InvalidRegex(Exception):
    """R022 invalid_regex: the portable contract rejected a pattern."""

    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


class RegexGroupOutOfRange(Exception):
    """R022 regex_group_out_of_range: a group the pattern does not declare."""


def regex_contract_requirement():
    """Name the contract and binding R022 pins, for a failure to report."""
    return (
        f"portable contract {REGEX_CONTRACT_VERSION} "
        "(the installed native yamaa.check and yamaa.domain services)"
    )


def require_regex_binding():
    """Import the portable binding on first use, or fail rather than substitute another."""
    global _portable_binding, _PortableRegexError
    global _portable_group_count, _portable_compile, _portable_full_match
    if _portable_binding is None:
        try:
            (
                binding_error,
                binding_group_count,
                binding_compile,
                binding_full_match,
            ) = (
                _native_regex.RegexError,
                _native_regex.capture_group_count,
                _native_regex.compile_pattern,
                _native_regex.full_match,
            )
        except ImportError:
            _portable_binding = False
        else:
            _PortableRegexError = binding_error
            _portable_group_count = binding_group_count
            _portable_compile = binding_compile
            _portable_full_match = binding_full_match
            _portable_binding = True
    if not _portable_binding:
        raise RegexBindingUnavailable(
            "unsupported_regex_engine under R022: this validator requires "
            f"{regex_contract_requirement()} and does not fall back to "
            "another regular-expression library"
        )


def compile_regex(pattern):
    """Compile one R022 pattern source through the portable contract."""
    require_regex_binding()
    if not isinstance(pattern, str):
        raise InvalidRegex(f"pattern must be a string, got {type(pattern).__name__}")
    try:
        return _portable_compile(pattern)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


def anchored_regex_source(pattern):
    """Return the full-match form R022 gives the `pattern` keyword."""
    return "^(?:" + pattern + ")$"


def regex_search(pattern, subject):
    """Return the leftmost match of an R022 pattern, or None."""
    return compile_regex(pattern).search(subject)


def regex_full_match(pattern, subject):
    """Return whether an R022 pattern matches the whole subject."""
    if not isinstance(subject, str):
        return False
    require_regex_binding()
    try:
        return _portable_full_match(pattern, subject)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


def regex_capture_group_count(pattern):
    """Count the capturing groups an accepted R022 pattern declares.

    Groups are numbered by the order of their opening parenthesis, counting
    only capturing groups: `(?:`, lookaround, and a parenthesis inside a
    character class contribute no number, while `(?<name>` does. The binding
    answers an out-of-range group index exactly as it answers a group the
    match did not enter, so the count is read from the pattern source the
    contract has already accepted.
    """
    require_regex_binding()
    try:
        return _portable_group_count(pattern)
    except _PortableRegexError as exc:
        raise InvalidRegex(exc.reason) from exc


REGEX_NO_MATCH = object()


def regex_extract(pattern, subject, group=0):
    """Return the R022 `str_extract` result for one pattern and subject.

    REGEX_NO_MATCH when the pattern matched nowhere, None when the match did
    not enter the requested group, and the matched text otherwise.
    """
    declared = regex_capture_group_count(pattern)
    if not isinstance(group, int) or group < 0 or group > declared:
        raise RegexGroupOutOfRange(
            f"group {group!r} is outside the {declared} capturing "
            f"{'group' if declared == 1 else 'groups'} the pattern declares"
        )
    match = regex_search(pattern, subject)
    if match is None:
        return REGEX_NO_MATCH
    return match.group(group)


def regex_pattern_errors(pattern, path, full_match=False):
    """Return the R022 invalid_regex error a pattern produces, if any."""
    try:
        compile_regex(pattern)
        if full_match:
            compile_regex(anchored_regex_source(pattern))
    except InvalidRegex as exc:
        return [
            validation_diagnostic(
                path,
                "invalid_regex",
                f"the R022 contract rejects {pattern!r}: {exc.reason}",
                context={"pattern": pattern, "reason": exc.reason},
            )
        ]
    return []


def regex_group_errors(keyword, payload, path):
    """Return the R022 regex_group_out_of_range error for a str_extract."""
    if keyword != "str_extract" or not isinstance(payload, dict):
        return []
    pattern = payload.get("pattern")
    group = payload.get("group", 0)
    if not isinstance(pattern, str) or type(group) is not int:
        return []
    try:
        declared = regex_capture_group_count(pattern)
    except InvalidRegex:
        # The pattern's own rejection is reported where it is typed `regex`.
        return []
    if group < 0 or group > declared:
        return [
            validation_diagnostic(
                f"{path}.group",
                "regex_group_out_of_range",
                f"group {group} is outside the {declared} capturing "
                f"{'group' if declared == 1 else 'groups'} {pattern!r} declares",
                context={
                    "group": group,
                    "group_count": declared,
                    "pattern": pattern,
                },
            )
        ]
    return []


REGEX_CONFORMANCE_PATH = PurePosixPath("yaml/conformance/regex.yaml")
REGEX_FIXTURE_COVERS = {
    "anchors",
    "capture-groups",
    "dialect",
    "empty-match",
    "escaping",
    "search",
    "unicode",
    "unsupported",
}
# The categories R022 requires the shared fixtures to exercise. A fixture set
# that stops covering one of them stops being evidence for that behavior.
REGEX_FIXTURE_REQUIRED_COVERS = {
    "anchors",
    "capture-groups",
    "empty-match",
    "escaping",
    "unicode",
    "unsupported",
}
REGEX_FIXTURE_CASE_KEYS = {
    "id",
    "covers",
    "pattern",
    "subject",
    "schema_pattern",
    "matches",
    "str_extract",
    "invalid",
}


def validate_regex_conformance(root: Path):
    """Replay R022's shared fixtures against the portable contract binding.

    Every case records what all three consumers produce for one pattern and
    subject. Replaying them here keeps the fixtures, this validator, and the
    shared binding from drifting apart, and gives the R implementation a file
    whose expected values are already known to be reachable.
    """
    label = str(REGEX_CONFORMANCE_PATH)
    document_path = root / REGEX_CONFORMANCE_PATH
    if not document_path.exists():
        return [f"ERROR: {label}: missing R022 regular-expression fixtures"]

    try:
        with open(document_path, "r", encoding="utf-8") as handle:
            document = yaml.load(handle, Loader=UniqueKeyLoader)
    except Exception as exc:  # noqa: BLE001 - retain original fixture decode diagnostics
        return [f"ERROR: {label}: {exc}"]

    if not isinstance(document, dict):
        return [f"ERROR: {label}: expected a mapping"]

    errors = []
    if document.get("contract") != REGEX_CONTRACT:
        errors.append(
            f"ERROR: {label}: contract {document.get('contract')!r} does not "
            f"name the portable contract this validator replays, "
            f"{REGEX_CONTRACT!r}"
        )
    if document.get("contract_version") != REGEX_CONTRACT_VERSION:
        errors.append(
            f"ERROR: {label}: contract_version "
            f"{document.get('contract_version')!r} is not the contract "
            f"version this validator replays, {REGEX_CONTRACT_VERSION!r}"
        )
    if "engine" in document:
        errors.append(f"ERROR: {label}: the portable contract pins no engine")

    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        errors.append(f"ERROR: {label}: cases must be a non-empty list")
        return errors

    seen = set()
    covered = set()
    for index, case in enumerate(cases):
        case_label = f"{label}: cases[{index}]"
        if not isinstance(case, dict):
            errors.append(f"ERROR: {case_label}: expected a mapping")
            continue
        case_id = case.get("id")
        if not isinstance(case_id, str) or not case_id:
            errors.append(f"ERROR: {case_label}: id must be a non-empty string")
            continue
        case_label = f"{label}: {case_id}"
        if case_id in seen:
            errors.append(f"ERROR: {case_label}: duplicate case id")
            continue
        seen.add(case_id)

        unknown = sorted(set(case) - REGEX_FIXTURE_CASE_KEYS)
        if unknown:
            errors.append(f"ERROR: {case_label}: unknown keys {unknown}")

        covers = case.get("covers")
        if not isinstance(covers, list) or not covers:
            errors.append(f"ERROR: {case_label}: covers must be a non-empty list")
        else:
            outside = sorted(set(covers) - REGEX_FIXTURE_COVERS)
            if outside:
                errors.append(f"ERROR: {case_label}: unknown covers {outside}")
            covered.update(covers)

        pattern = case.get("pattern")
        if not isinstance(pattern, str):
            errors.append(f"ERROR: {case_label}: pattern must be a string")
            continue

        errors.extend(_regex_fixture_case_errors(case, case_label, pattern))

    missing = sorted(REGEX_FIXTURE_REQUIRED_COVERS - covered)
    if missing:
        errors.append(f"ERROR: {label}: no case covers {missing}")
    return errors


def _regex_fixture_case_errors(case, case_label, pattern):
    """Compare one fixture case with what the portable binding produces."""
    errors = []
    if case.get("invalid") is True:
        for key in ("subject", "schema_pattern", "matches", "str_extract"):
            if key in case:
                errors.append(
                    f"ERROR: {case_label}: a rejected pattern records no {key}"
                )
        try:
            compile_regex(pattern)
        except InvalidRegex:
            return errors
        errors.append(
            f"ERROR: {case_label}: the portable binding accepts pattern "
            f"{pattern!r} the fixture records as invalid"
        )
        return errors

    if "invalid" in case:
        errors.append(f"ERROR: {case_label}: invalid must be true or absent")
    subject = case.get("subject")
    if not isinstance(subject, str):
        errors.append(f"ERROR: {case_label}: subject must be a string")
        return errors

    try:
        actual_full = regex_full_match(pattern, subject)
        actual_search = regex_search(pattern, subject) is not None
    except InvalidRegex as exc:
        errors.append(
            f"ERROR: {case_label}: the portable binding rejects pattern "
            f"{pattern!r}: {exc.reason}"
        )
        return errors

    for key, actual in (
        ("schema_pattern", actual_full),
        ("matches", actual_search),
    ):
        recorded = case.get(key)
        if type(recorded) is not bool:
            errors.append(f"ERROR: {case_label}: {key} must be true or false")
        elif recorded is not actual:
            errors.append(
                f"ERROR: {case_label}: {key} records {recorded} but the "
                f"portable binding produces {actual}"
            )

    errors.extend(_regex_fixture_extract_errors(case, case_label, pattern, subject))
    return errors


def _regex_fixture_extract_errors(case, case_label, pattern, subject):
    """Compare one fixture's recorded str_extract result with the engine."""
    if "str_extract" not in case:
        return [f"ERROR: {case_label}: str_extract is required"]
    recorded = case["str_extract"]
    if recorded is None:
        group, expected = 0, REGEX_NO_MATCH
    elif isinstance(recorded, dict) and set(recorded) == {"group", "value"}:
        group = recorded["group"]
        expected = recorded["value"]
        if type(group) is not int:
            return [f"ERROR: {case_label}: str_extract group must be an int"]
        if expected is not None and not isinstance(expected, str):
            return [f"ERROR: {case_label}: str_extract value must be a string or null"]
    else:
        return [
            (
                f"ERROR: {case_label}: str_extract must be null or a mapping of "
                f"group and value"
            )
        ]

    try:
        actual = regex_extract(pattern, subject, group)
    except (InvalidRegex, RegexGroupOutOfRange) as exc:
        return [f"ERROR: {case_label}: str_extract group {group}: {exc}"]

    if actual is REGEX_NO_MATCH and expected is REGEX_NO_MATCH:
        return []
    if actual is REGEX_NO_MATCH:
        return [
            (
                f"ERROR: {case_label}: str_extract records a match but the "
                f"portable binding finds none"
            )
        ]
    if expected is REGEX_NO_MATCH:
        return [
            (
                f"ERROR: {case_label}: str_extract records no match but the "
                f"portable binding matches"
            )
        ]
    if actual != expected:
        return [
            (
                f"ERROR: {case_label}: str_extract group {group} records "
                f"{expected!r} but the portable binding produces {actual!r}"
            )
        ]
    return []
