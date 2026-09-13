from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.expressions import (
    MappingResolver,
    TemplateError,
    TemplatePart,
    ascii_lower,
    ascii_upper,
    evaluate_expression,
    parse_template,
    template_identifiers,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import (
    MISSING,
    ConditionResult,
    TypedColumn,
    TypedTable,
    ValueResult,
)
from yamaa.regex import (
    NO_MATCH,
    REGEX_ENGINE_CRATE,
    REGEX_ENGINE_CRATE_VERSION,
    REGEX_FLAGS,
    RegexError,
    capture_group_count,
    compile_pattern,
    full_match,
    regex_extract,
)
from yamaa.specification.models import Column
from yamaa.verification import DeclarationError, check_column

REPOSITORY_ROOT = Path(__file__).parents[3]
GRAMMAR = yaml.safe_load(
    (REPOSITORY_ROOT / "yaml/grammar/string-template.yaml").read_text(encoding="ascii")
)
CONFORMANCE = yaml.safe_load(
    (REPOSITORY_ROOT / "yaml/conformance/regex.yaml").read_text(encoding="ascii")
)
CASES = CONFORMANCE["cases"]
KEY_COLUMNS = (
    TypedColumn(name="KEY", type="str"),
    TypedColumn(name="VALUE", type="str"),
)


def _quote(value: str) -> str:
    return f"'{value.replace(chr(39), chr(39) * 2)}'"


def _shape(parts: tuple[TemplatePart, ...]) -> str:
    rendered = ["template"]
    for part in parts:
        if part["kind"] == "text":
            rendered.append(f"(text {_quote(part['value'])})")
        else:
            rendered.append(f"(placeholder {part['name']})")
    return f"({' '.join(rendered)})"


@pytest.mark.parametrize(
    "case",
    GRAMMAR["cases"],
    ids=[case["id"] for case in GRAMMAR["cases"]],
)
def test_template_parser_matches_the_shared_r012_contract(
    case: dict[str, object],
) -> None:
    text = case["text"]
    assert isinstance(text, str)
    if case["parse"] == "accept":
        parts = parse_template(text)
        expected_shape = case["shape"]
        assert isinstance(expected_shape, str)
        assert _shape(parts) == " ".join(expected_shape.split())
        assert sorted(template_identifiers(parts)) == case["identifiers"]
        return
    with pytest.raises(TemplateError) as caught:
        parse_template(text)
    assert caught.value.condition == case["condition"]


def test_the_pinned_engine_is_the_one_the_fixtures_name() -> None:
    # R022-26: the fixtures and this consumer must name one engine, or a
    # replay proves nothing about the engine the language pins.
    assert CONFORMANCE["engine"] == {
        "crate": REGEX_ENGINE_CRATE,
        "crate_version": REGEX_ENGINE_CRATE_VERSION,
        "flags": REGEX_FLAGS,
    }


def _matches_column(pattern: str, subject: str) -> bool:
    """Replay one case through the real R009 `matches` verification."""
    column = Column.model_validate(
        {
            "name": "VALUE",
            "type": "str",
            "verifications": [{"matches": {"pattern": pattern}}],
        }
    )
    table = TypedTable(
        columns=KEY_COLUMNS,
        frame=frame_from_values(KEY_COLUMNS, [["k1", subject]]).frame,
    )
    return not check_column(table, column, ["KEY"])


def _extract(pattern: str, subject: str, group: int) -> object:
    """Replay one case through the real `str_extract` expression."""
    return evaluate_expression(
        {
            "str_extract": {
                "source": "SUBJECT",
                "pattern": pattern,
                "group": group,
            }
        },
        MappingResolver({"SUBJECT": subject}),
    )


@pytest.mark.parametrize("case", CASES, ids=[case["id"] for case in CASES])
def test_every_shared_regex_vector_replays_in_all_three_consumers(
    case: dict[str, object],
) -> None:
    pattern = case["pattern"]
    assert isinstance(pattern, str)

    if case.get("invalid") is True:
        # R022-27: one rejection, identically, in all three consumers.
        with pytest.raises(RegexError):
            compile_pattern(pattern)
        with pytest.raises(RegexError):
            full_match(pattern, "")
        with pytest.raises(DeclarationError) as declaration:
            _matches_column(pattern, "")
        assert declaration.value.condition == "invalid_regex"
        extracted = _extract(pattern, "", 0)
        assert isinstance(extracted, ConditionResult)
        assert extracted.condition.condition == "invalid_regex"
        return

    subject = case["subject"]
    assert isinstance(subject, str)

    assert full_match(pattern, subject) is case["schema_pattern"]
    assert _matches_column(pattern, subject) is case["matches"]

    recorded = case["str_extract"]
    if recorded is None:
        group, expected = 0, NO_MATCH
    else:
        group, expected = recorded["group"], recorded["value"]

    assert regex_extract(pattern, subject, group) == expected

    result = _extract(pattern, subject, group)
    if expected is NO_MATCH:
        # No `no_match` handler is declared, so the condition is fatal.
        assert isinstance(result, ConditionResult)
        assert result.condition.applicable_handler == "no_match"
        return
    assert isinstance(result, ValueResult)
    assert result.value == (MISSING if expected is None else expected)


def test_the_schema_descriptor_reads_patterns_through_the_pinned_engine() -> None:
    from yamaa.specification import schema

    # The R006 descriptor consumer is the same binding the vectors replay
    # through, so a dialect difference cannot hide behind a second import.
    assert schema.full_match is full_match
    assert schema.compile_pattern is compile_pattern


@pytest.mark.parametrize(
    ("pattern", "expected"),
    [
        (r"^CATH-([^-]+)-([0-9]{4})$", 2),
        (r"(?:a)(b)", 1),
        (r"(?<year>[0-9]{4})", 1),
        (r"(?=a)(b)", 1),
        (r"[(]", 0),
        (r"\((a)\)", 1),
        (r"a", 0),
    ],
)
def test_capture_groups_are_numbered_by_opening_parenthesis(
    pattern: str, expected: int
) -> None:
    assert capture_group_count(pattern) == expected


def test_a_group_the_match_did_not_enter_is_missing_rather_than_no_match() -> None:
    # R022-22: the pattern matched, so `no_match` deliberately does not apply.
    result = evaluate_expression(
        {
            "str_extract": {
                "source": "SUBJECT",
                "pattern": "(a)|(b)",
                "group": 1,
                "no_match": "NEVER",
            }
        },
        MappingResolver({"SUBJECT": "b"}),
    )

    assert result == ValueResult(value=MISSING)


def test_an_empty_match_returns_the_empty_string_rather_than_missing() -> None:
    # R022-23 and R019 keep the empty string and missing distinct.
    result = evaluate_expression(
        {"str_extract": {"source": "SUBJECT", "pattern": "a*", "group": 0}},
        MappingResolver({"SUBJECT": "xyz"}),
    )

    assert result == ValueResult(value="")


@pytest.mark.parametrize("group", [2, -1])
def test_a_group_outside_the_pattern_fails_validation(group: int) -> None:
    result = _extract(r"^CATH-([^-]+)-[0-9]{4}$", "CATH-UCSD-0001", group)

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "regex_group_out_of_range"
    assert result.condition.requirement == "R022-28"
    assert result.condition.path_suffix == "group"
    assert result.condition.context["group_count"] == 1


@pytest.mark.parametrize(
    ("operation", "value", "expected"),
    [
        ("str_upper", "abcZ", "ABCZ"),
        ("str_lower", "ABCz", "abcz"),
        ("str_upper", "a1!_", "A1!_"),
        ("str_lower", "A1!_", "a1!_"),
        # R019-13: no one-to-many mapping, so scalar count is preserved.
        ("str_upper", "\u00df", "\u00df"),
        ("str_lower", "\u0130", "\u0130"),
        ("str_upper", "\u00e9", "\u00e9"),
        ("str_lower", "\u00c9", "\u00c9"),
    ],
)
def test_casing_is_the_ascii_substitution_and_nothing_else(
    operation: str, value: str, expected: str
) -> None:
    result = evaluate_expression(
        {operation: {"source": "VALUE"}}, MappingResolver({"VALUE": value})
    )

    assert result == ValueResult(value=expected)
    assert len(expected) == len(value)


@pytest.mark.parametrize(
    "value",
    ["\u00df", "\u0130", "\u0131", "\u212a", "\u00e9"],
)
def test_ascii_casing_leaves_every_non_ascii_scalar_alone(value: str) -> None:
    assert ascii_upper(value) == value
    assert ascii_lower(value) == value
    # A host routine changes each of these, which is why R019-13 forbids
    # inheriting one: it would fold, expand, or retitle the scalar.
    assert value.upper() != value or value.lower() != value


def test_ascii_casing_moves_only_the_ascii_letters_of_a_mixed_value() -> None:
    mixed = "Stra\u00dfe"

    assert ascii_upper(mixed) == "STRA\u00dfE"
    assert ascii_lower(mixed) == "stra\u00dfe"
    # A host uppercase would expand the sharp s into two scalars.
    assert len(ascii_upper(mixed)) == len(mixed)


@pytest.mark.parametrize("operation", ["str_upper", "str_lower", "str_extract"])
def test_a_missing_input_uses_the_declared_handler(operation: str) -> None:
    payload: dict[str, object] = {"source": "VALUE", "missing": "UNKNOWN"}
    if operation == "str_extract":
        payload |= {"pattern": "(.*)", "group": 1}

    result = evaluate_expression(
        {operation: payload}, MappingResolver({"VALUE": MISSING})
    )

    assert result == ValueResult(value="UNKNOWN", handled_by="missing")


@pytest.mark.parametrize("operation", ["str_upper", "str_lower", "str_template"])
def test_an_undeclared_missing_handler_is_fatal(operation: str) -> None:
    payload: object = {"source": "VALUE"}
    if operation == "str_template":
        payload = {"template": "{VALUE}"}

    result = evaluate_expression(
        {operation: payload}, MappingResolver({"VALUE": MISSING})
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "missing_input"
    assert result.condition.applicable_handler == "missing"
    assert result.condition.requirement == "R007-49"


def test_an_unhandled_no_match_is_distinct_from_an_unhandled_missing() -> None:
    # R008-6: `no_match` fires only when the input is present, so the two
    # conditions stay distinct rather than collapsing into one.
    result = _extract("^CATH-", "SUBJECT-0003", 0)

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unmatched_pattern"
    assert result.condition.applicable_handler == "no_match"
    assert result.condition.context == {
        "value": "SUBJECT-0003",
        "pattern": "^CATH-",
    }


@pytest.mark.parametrize("operation", ["str_upper", "str_lower"])
def test_a_non_string_source_is_refused_rather_than_converted(
    operation: str,
) -> None:
    result = evaluate_expression(
        {operation: {"source": "VALUE"}}, MappingResolver({"VALUE": 7})
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "incompatible_input_type"
    assert result.condition.requirement == "R007-24"
    assert result.condition.context == {
        "source": "VALUE",
        "expected": "str",
        "actual": "int",
    }


@pytest.mark.parametrize(
    ("template", "expected"),
    [
        ("{SITE}:{SUBJ}", "UCSD:0001"),
        ("{SITE}-{SITE}", "UCSD-UCSD"),
        ("", ""),
        ("no placeholder", "no placeholder"),
        ("{{literal}}", "{literal}"),
        ("{{{SITE}}}", "{UCSD}"),
    ],
)
def test_a_template_interpolates_exactly_what_it_scans(
    template: str, expected: str
) -> None:
    result = evaluate_expression(
        {"str_template": {"template": template}},
        MappingResolver({"SITE": "UCSD", "SUBJ": "0001"}),
    )

    assert result == ValueResult(value=expected)


def test_the_bare_template_shorthand_carries_no_missing_handler() -> None:
    # R012-3: the shorthand expands to `{template: ...}` and adds nothing.
    rendered = evaluate_expression(
        {"str_template": "{SITE}"}, MappingResolver({"SITE": "UCSD"})
    )
    fatal = evaluate_expression(
        {"str_template": "{SITE}"}, MappingResolver({"SITE": MISSING})
    )

    assert rendered == ValueResult(value="UCSD")
    assert isinstance(fatal, ConditionResult)
    assert fatal.condition.condition == "missing_input"


def test_one_missing_placeholder_answers_the_whole_template() -> None:
    result = evaluate_expression(
        {"str_template": {"template": "{A}{B}", "missing": "UNKNOWN"}},
        MappingResolver({"A": "value", "B": MISSING}),
    )

    assert result == ValueResult(value="UNKNOWN", handled_by="missing")


def test_a_template_outside_the_grammar_names_its_placeholder() -> None:
    result = evaluate_expression(
        {"str_template": {"template": "{A + B}"}},
        MappingResolver({"A": "1", "B": "2"}),
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "invalid_string_template"
    assert result.condition.requirement == "R012-16"
    assert result.condition.context == {
        "reason": "invalid_placeholder",
        "placeholder": "A + B",
    }


def test_concatenation_places_literals_beside_sources() -> None:
    result = evaluate_expression(
        {
            "str_concat": {
                "sources": [{"source": "SITE"}, {"literal": "-"}, {"source": "SUBJ"}]
            }
        },
        MappingResolver({"SITE": "UCSD", "SUBJ": "0001"}),
    )

    assert result == ValueResult(value="UCSD-0001")


def test_a_handler_inside_a_nested_source_is_observed_at_its_own_path() -> None:
    # R008-21 counts every handler path, including one R007-3 lets nest.
    result = evaluate_expression(
        {
            "str_concat": {
                "sources": [
                    {"source": {"variable": "ABSENT", "missing": "NA"}},
                    {"literal": "!"},
                ]
            }
        },
        MappingResolver({}),
    )

    assert isinstance(result, ValueResult)
    assert result.value == "NA!"
    assert [(item.path, item.handler) for item in result.observations] == [
        ("sources[0].source", "missing")
    ]


def test_a_missing_concatenation_input_uses_the_declared_handler() -> None:
    result = evaluate_expression(
        {
            "str_concat": {
                "sources": [{"source": "SITE"}, {"source": "SUBJ"}],
                "missing": "UNKNOWN",
            }
        },
        MappingResolver({"SITE": "UCSD", "SUBJ": MISSING}),
    )

    assert isinstance(result, ValueResult)
    assert result.value == "UNKNOWN"
    assert result.handled_by == "missing"
