from __future__ import annotations

import re
import textwrap
from pathlib import Path

import pytest

from yamaa.style import (
    FINDINGS,
    check_file,
    field_orders,
    fix_file,
    is_specification,
    specification_files,
)
from yamaa.style.__main__ import main
from yamaa.style._fix import proved

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
CONTRACT = REPOSITORY_ROOT / "rules" / "specification" / "style.md"
ORDERS = field_orders(SCHEMA_ROOT)

CANONICAL = """\
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  EX: input/ex.csv
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTSDT]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    order_by: [EX.EXSTDTC]
    keep: first

columns:
  # The study identifier as collected.
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: TRTSDT
    type: str
    label: Date of First Exposure
    derivation:
      case:
        - when: "FIRSTDOSE.EXSTDTC IS NULL"
          then: {literal: null}
        - otherwise: FIRSTDOSE.EXSTDTC

verifications:
  - row_count: {min: 1}
  - unique: [USUBJID]
"""


def _spec(tmp_path: Path, text: str, name: str = "spec.yaml") -> Path:
    path = tmp_path / name
    path.write_text(textwrap.dedent(text), encoding="utf-8")
    return path


def _names(path: Path) -> list[str]:
    return [finding.name for finding in check_file(path, orders=ORDERS)]


def _fix(path: Path) -> tuple[str, list[str]]:
    before = path.read_text(encoding="utf-8")
    changed, remaining = fix_file(path, orders=ORDERS)
    after = path.read_text(encoding="utf-8")
    assert changed == (after != before)
    assert proved(before, after)
    # REQ-1286: fixing the fixed file changes nothing.
    assert fix_file(path, orders=ORDERS)[0] is False
    return after, [finding.name for finding in remaining]


def test_canonical_specification_has_no_findings(tmp_path: Path) -> None:
    assert _names(_spec(tmp_path, CANONICAL)) == []


def test_finding_carries_file_line_column_and_requirement(tmp_path: Path) -> None:
    path = _spec(
        tmp_path, CANONICAL.replace("    dataset: EX\n", "    dataset: EX\n\n\n")
    )
    finding = check_file(path, orders=ORDERS)[0]
    assert finding.name == "blank_lines"
    assert (finding.line, finding.column) == (17, 1)
    assert finding.requirement == "REQ-1284"
    assert finding.render().startswith(f"{path}:17:1: blank_lines ")


def test_root_fields_follow_the_schema_order(tmp_path: Path) -> None:
    moved = CANONICAL.replace(
        "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID, TRTSDT]\n\n", ""
    ).replace(
        "columns:\n  # The",
        "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID, TRTSDT]\n\n"
        "columns:\n  # The",
    )
    path = _spec(tmp_path, moved)
    finding = check_file(path, orders=ORDERS)[0]
    assert (finding.name, finding.spec_path) == ("field_order", "output")
    assert finding.message == "write `output` before `intermediates`"
    after, remaining = _fix(path)
    assert after == CANONICAL
    assert remaining == []


def test_parents_and_filter_belong_to_the_header(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        """\
        schema_version: "1.0"
        domain: ADSL
        parents: base.yaml
        keys: [USUBJID]
        input:
          DM: input/dm.csv

        output:
          path: adsl.csv
          columns: [USUBJID]

        columns:
          - name: USUBJID
            type: str
            label: Unique Subject Identifier
            derivation: DM.USUBJID

        filter: "DM.ARMCD IS NOT NULL"
        """,
    )
    # One finding per mapping: the first field written out of order.
    assert _names(path) == ["field_order"]
    after, remaining = _fix(path)
    assert remaining == []
    assert after.startswith(
        'schema_version: "1.0"\nparents: base.yaml\ndomain: ADSL\n'
        "keys: [USUBJID]\ninput:\n  DM: input/dm.csv\n"
        'filter: "DM.ARMCD IS NOT NULL"\n\noutput:\n'
    )


def test_entry_fields_move_with_the_list_dash(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "  - name: USUBJID\n    type: str\n",
            "  - type: str\n    name: USUBJID\n",
        ).replace(
            "    order_by: [EX.EXSTDTC]\n    keep: first\n",
            "    keep: first\n    order_by: [EX.EXSTDTC]\n",
        ),
    )
    assert _names(path) == ["field_order", "field_order"]
    after, remaining = _fix(path)
    assert after == CANONICAL
    assert remaining == []


def test_an_entry_with_a_blank_line_inside_is_not_reordered(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "  - name: USUBJID\n    type: str\n",
            "  - type: str\n\n    name: USUBJID\n",
        ),
    )
    _, remaining = _fix(path)
    assert remaining == ["field_order"]


def test_multiline_entries_are_separated_by_one_blank_line(tmp_path: Path) -> None:
    packed = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n", "    derivation: DM.STUDYID\n"
    )
    path = _spec(tmp_path, packed)
    finding = check_file(path, orders=ORDERS)[0]
    assert (finding.name, finding.spec_path, finding.line) == (
        "entry_spacing",
        "columns[1]",
        25,
    )
    after, remaining = _fix(path)
    assert after == CANONICAL
    assert remaining == []


def test_the_blank_line_goes_above_the_entry_comment(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "    keep: first\n\ncolumns:\n",
            "    keep: first\n  # The last dose.\n  - id: LASTDOSE\n    dataset: EX\n"
            "    order_by: [EX.EXSTDTC]\n    keep: last\n\ncolumns:\n",
        ),
    )
    assert _names(path) == ["entry_spacing"]
    after, _ = _fix(path)
    assert "    keep: first\n\n  # The last dose.\n  - id: LASTDOSE\n" in after


def test_single_line_entries_may_stand_together(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "columns:\n  # The study identifier as collected.\n",
            "columns:\n  - {name: SITEID, type: str, label: Site, derivation: DM.SITEID}\n"
            "  - {name: ARM, type: str, label: Arm, derivation: DM.ARM}\n\n",
        ),
    )
    assert _names(path) == []


def test_sections_need_a_blank_line_and_the_header_does_not(tmp_path: Path) -> None:
    path = _spec(
        tmp_path, CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:")
    )
    finding = check_file(path, orders=ORDERS)[0]
    assert (finding.name, finding.spec_path) == ("root_spacing", "output")
    after, _ = _fix(path)
    assert after == CANONICAL
    spaced = CANONICAL.replace("domain: ADSL\n", "\ndomain: ADSL\n\n")
    assert _names(_spec(tmp_path, spaced, "spaced.yaml")) == []


@pytest.mark.parametrize(
    ("text", "message"),
    [
        ("\n" + CANONICAL, "remove the leading blank line"),
        (CANONICAL + "\n", "remove the blank line at the end"),
        (CANONICAL.rstrip("\n"), "end the file with a line break"),
        (
            CANONICAL.replace("columns:\n  # The", "columns:\n\n  # The"),
            "remove the blank line after the key that opens this block",
        ),
        (
            CANONICAL.replace("\ncolumns:\n", "\n\ncolumns:\n"),
            "remove the second blank line",
        ),
    ],
)
def test_blank_lines(tmp_path: Path, text: str, message: str) -> None:
    path = tmp_path / "spec.yaml"
    path.write_text(text, encoding="utf-8")
    findings = check_file(path, orders=ORDERS)
    assert [(item.name, item.message) for item in findings] == [
        ("blank_lines", message)
    ]
    after, remaining = _fix(path)
    assert after == CANONICAL
    assert remaining == []


def test_blank_lines_inside_a_block_scalar_are_value_text(tmp_path: Path) -> None:
    text = CANONICAL.replace(
        "    derivation: DM.STUDYID\n",
        "    derivation: DM.STUDYID\n    metadata:\n      note: |\n"
        "        First paragraph.\n\n\n        Second paragraph.\n",
    )
    assert _names(_spec(tmp_path, text)) == []


def test_a_fix_that_would_change_a_value_is_refused(tmp_path: Path) -> None:
    # The blank line the fix would insert before USUBJID becomes part of the
    # `|+` scalar, which keeps its trailing line breaks, so the proof fails.
    text = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n",
        "    derivation: DM.STUDYID\n    metadata:\n      note: |+\n        kept\n",
    )
    path = _spec(tmp_path, text)
    assert _names(path) == ["entry_spacing"]
    changed, remaining = fix_file(path, orders=ORDERS)
    assert changed is False
    assert [finding.name for finding in remaining] == ["entry_spacing"]
    assert path.read_text(encoding="utf-8") == text


def test_long_flow_lists_wrap_after_the_bracket(tmp_path: Path) -> None:
    columns = ", ".join(f"COLUMN{index:02d}" for index in range(12))
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "  columns: [STUDYID, USUBJID, TRTSDT]",
            f"  columns: [STUDYID, USUBJID, TRTSDT, {columns}]",
        ),
    )
    assert _names(path) == ["line_width"]
    after, remaining = _fix(path)
    assert remaining == []
    assert (
        "  columns: [STUDYID, USUBJID, TRTSDT, COLUMN00, COLUMN01, COLUMN02, COLUMN03,\n"
        "            COLUMN04, COLUMN05, COLUMN06, COLUMN07, COLUMN08, COLUMN09,\n"
        "            COLUMN10, COLUMN11]\n" in after
    )


def test_long_predicates_continue_after_a_comma_or_before_a_connective(
    tmp_path: Path,
) -> None:
    predicate = (
        "FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT IN ('PLACEBO', 'DRUG A')"
        " AND FIRSTDOSE.EXDOSE > 0"
    )
    path = _spec(
        tmp_path,
        CANONICAL.replace('"FIRSTDOSE.EXSTDTC IS NULL"', f'"{predicate}"'),
    )
    assert _names(path) == ["line_width"]
    after, remaining = _fix(path)
    assert remaining == []
    assert (
        "        - when: \"FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT IN ('PLACEBO',\n"
        "            'DRUG A') AND FIRSTDOSE.EXDOSE > 0\"\n" in after
    )
    spaced = "FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT = 'PLACEBO'" + (
        " AND FIRSTDOSE.EXDOSE > 0 AND FIRSTDOSE.EXDOSU = 'mg'"
    )
    path = _spec(
        tmp_path,
        CANONICAL.replace('"FIRSTDOSE.EXSTDTC IS NULL"', f'"{spaced}"'),
        "connective.yaml",
    )
    after, remaining = _fix(path)
    assert remaining == []
    assert (
        "        - when: \"FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT = 'PLACEBO'\n"
        "            AND FIRSTDOSE.EXDOSE > 0 AND FIRSTDOSE.EXDOSU = 'mg'\"\n" in after
    )


def test_long_flow_mappings_become_blocks_but_literals_stay(tmp_path: Path) -> None:
    derivation = (
        "{date_impute: {source: FIRSTDOSE.EXSTDTC, month: 1, day: last, missing: null}}"
    )
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "    derivation: DM.USUBJID\n", f"    derivation: {derivation}\n"
        ),
    )
    after, remaining = _fix(path)
    assert remaining == []
    assert (
        "    derivation:\n      date_impute: {source: FIRSTDOSE.EXSTDTC, month: 1,"
        in after
        or "    derivation:\n      date_impute:\n        source: FIRSTDOSE.EXSTDTC\n"
        in after
    )
    assert "then: {literal: null}" in after


def test_a_long_comment_is_reported_and_left_to_its_author(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "  # The study identifier as collected.",
            "  # The study identifier as collected, copied without change from"
            " the demographics domain.",
        ),
    )
    _, remaining = _fix(path)
    assert remaining == ["line_width"]


def test_literal_and_source_spellings(tmp_path: Path) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "          then: {literal: null}\n        - otherwise: FIRSTDOSE.EXSTDTC\n",
            "          then:\n            literal: null\n"
            "        - otherwise: {source: FIRSTDOSE.EXSTDTC}\n",
        ).replace(
            "    derivation: DM.USUBJID\n",
            "    derivation:\n      source: DM.USUBJID\n",
        ),
    )
    findings = check_file(path, orders=ORDERS)
    assert [(item.name, item.spec_path) for item in findings] == [
        ("source_form", "columns[1].derivation"),
        ("literal_form", "columns[2].derivation.case[0].then"),
        ("source_form", "columns[2].derivation.case[1].otherwise"),
    ]
    # REQ-1286: a spelling is corrected by its author, not by a fix.
    _, remaining = _fix(path)
    assert remaining == ["source_form", "literal_form", "source_form"]


def test_sources_outside_the_shorthand_positions_keep_the_mapping(
    tmp_path: Path,
) -> None:
    path = _spec(
        tmp_path,
        CANONICAL.replace(
            "    derivation: DM.USUBJID\n",
            "    derivation:\n      upcase:\n        source: DM.USUBJID\n",
        ).replace(
            "    derivation: DM.STUDYID\n",
            '    derivation:\n      source: {variable: DM.STUDYID, filter: "DM.X = 1"}\n',
        ),
    )
    assert _names(path) == []


def test_a_suppression_with_a_reason_covers_its_entry(tmp_path: Path) -> None:
    packed = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n", "    derivation: DM.STUDYID\n"
    )
    allowed = packed.replace(
        "  - name: USUBJID",
        "  # yamaa-style: allow entry_spacing -- kept beside STUDYID\n  - name: USUBJID",
    )
    assert _names(_spec(tmp_path, allowed)) == []
    path = _spec(tmp_path, allowed, "fixed.yaml")
    assert _fix(path)[0] == allowed


@pytest.mark.parametrize(
    "comment",
    [
        "# yamaa-style: allow entry_spacing",
        "# yamaa-style: allow entry_spacing --",
        "# yamaa-style: allow spacing -- no such finding",
        "# yamaa-style: allow invalid_suppression -- cannot be suppressed",
        "# yamaa-style: ignore entry_spacing -- wrong verb",
    ],
)
def test_an_invalid_suppression_is_reported(tmp_path: Path, comment: str) -> None:
    packed = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n", "    derivation: DM.STUDYID\n"
    )
    text = packed.replace("  - name: USUBJID", f"  {comment}\n  - name: USUBJID")
    assert _names(_spec(tmp_path, text)) == ["invalid_suppression", "entry_spacing"]


def test_the_proof_keeps_types_and_comments_apart() -> None:
    assert proved("a: 1\nb: x\n", "b: x\na: 1\n")
    assert not proved("a: 1\n", 'a: "1"\n')
    assert not proved("a: 1\n", "a: 1.0\n")
    assert not proved("a: true\n", "a: 1\n")
    assert not proved("# note\na: 1\n", "a: 1\n")


def test_directories_expand_to_specifications_only(tmp_path: Path) -> None:
    _spec(tmp_path, CANONICAL, "adsl.yaml")
    _spec(tmp_path, 'schema_version: "1.0"\ndefine_version: "2.1.11"\n', "define.yaml")
    _spec(tmp_path, "data_roots: [data]\n", "yamaa-project.yaml")
    hidden = tmp_path / ".cache"
    hidden.mkdir()
    _spec(hidden, CANONICAL, "copy.yaml")
    root_fields = ORDERS["root_class"]
    assert [path.name for path in specification_files([tmp_path], root_fields)] == [
        "adsl.yaml"
    ]
    assert not is_specification(tmp_path / "define.yaml", root_fields)


def test_command_line_exit_status(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    schema = ["--schema-root", str(SCHEMA_ROOT)]
    clean = _spec(tmp_path, CANONICAL, "clean.yaml")
    packed = _spec(
        tmp_path,
        CANONICAL.replace(
            "    derivation: DM.STUDYID\n\n", "    derivation: DM.STUDYID\n"
        ),
        "packed.yaml",
    )
    broken = _spec(tmp_path, "a: [\n", "broken.yaml")
    assert main([*schema, str(clean)]) == 0
    assert main([*schema, str(packed)]) == 1
    assert "entry_spacing" in capsys.readouterr().out
    assert main([*schema, "--ignore", "entry_spacing", str(packed)]) == 0
    assert main([*schema, str(broken)]) == 2
    assert main([*schema, "--fix", str(packed)]) == 0
    assert packed.read_text(encoding="utf-8") == CANONICAL
    with pytest.raises(SystemExit):
        main([*schema, "--select", "no_such_finding", str(clean)])


def test_every_finding_is_named_by_its_contract_requirement() -> None:
    contract = CONTRACT.read_text(encoding="ascii")
    for name, requirement in FINDINGS.items():
        match = re.search(
            rf"\*\*{requirement}\.\*\*(.*?)(?=<a id=|\Z)", contract, re.DOTALL
        )
        assert match is not None, requirement
        assert f"`{name}`" in match[1], (name, requirement)


REPEATED = """\
schema_version: "1.0"
domain: DM
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv

output:
  path: dm.csv
  columns: [STUDYID, USUBJID, DOMAIN]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier

  - name: USUBJID
    type: str
    label: Unique Subject Identifier

  - name: DOMAIN
    type: str
    label: Domain Abbreviation

rows:
  - id: a
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: {literal: DM}

  - id: b
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: {literal: DM}
"""


def test_repeated_row_derivations_move_to_column_level(
    tmp_path: Path,
) -> None:
    path = _spec(tmp_path, REPEATED)
    findings = check_file(path, orders=ORDERS)
    assert [(item.name, item.spec_path, item.requirement) for item in findings] == [
        ("repeated_row_derivation", "rows[0].derivations.STUDYID", "REQ-1288"),
        ("repeated_row_derivation", "rows[0].derivations.USUBJID", "REQ-1288"),
        ("repeated_row_derivation", "rows[0].derivations.DOMAIN", "REQ-1288"),
    ]
    first = findings[0]
    assert (first.line, first.column) == (27, 7)
    assert "move it to columns[].derivation" in first.message
    assert first.render().startswith(f"{path}:27:7: repeated_row_derivation ")
    # REQ-1286: authoring lints rewrite the value, so no fix is offered.
    changed, remaining = fix_file(path, orders=ORDERS)
    assert changed is False
    assert [item.name for item in remaining] == [
        "repeated_row_derivation",
        "repeated_row_derivation",
        "repeated_row_derivation",
    ]


def test_repeated_row_derivation_needs_every_template(tmp_path: Path) -> None:
    partial = REPEATED.replace(
        "  - id: b\n    derivations:\n      STUDYID: DM.STUDYID\n",
        "  - id: b\n    derivations:\n",
    )
    names = [
        (item.name, item.spec_path)
        for item in check_file(_spec(tmp_path, partial), orders=ORDERS)
    ]
    # STUDYID is derived in only one template, so it stays; USUBJID and
    # DOMAIN are still uniform and still reported.
    assert ("repeated_row_derivation", "rows[0].derivations.STUDYID") not in names
    assert ("repeated_row_derivation", "rows[0].derivations.USUBJID") in names
    assert ("repeated_row_derivation", "rows[0].derivations.DOMAIN") in names


def test_repeated_row_derivation_skips_row_phase_work(tmp_path: Path) -> None:
    aggregate = REPEATED.replace(
        "      DOMAIN: {literal: DM}\n\n  - id: b",
        "      DOMAIN:\n        aggregate:\n"
        '          filter: "DM.X = 1"\n'
        '          expr: "ONLY(DM.Y)"\n\n  - id: b',
    ).replace(
        "      DOMAIN: {literal: DM}\n",
        "      DOMAIN:\n        aggregate:\n"
        '          filter: "DM.X = 1"\n'
        '          expr: "ONLY(DM.Y)"\n',
    )
    names = [
        (item.name, item.spec_path)
        for item in check_file(_spec(tmp_path, aggregate), orders=ORDERS)
    ]
    # STUDYID and USUBJID stay row-local and stay reported; the aggregate
    # is row-phase dependent and stays where it is written.
    assert ("repeated_row_derivation", "rows[0].derivations.STUDYID") in names
    assert ("repeated_row_derivation", "rows[0].derivations.DOMAIN") not in names
    intermediate = REPEATED.replace(
        "output:\n  path: dm.csv",
        "intermediates:\n  - id: PICK\n    dataset: DM\n"
        '    filter: "DM.X = 1"\n    no_match: null\n\noutput:\n  path: dm.csv',
    ).replace("DOMAIN: {literal: DM}", "DOMAIN: PICK.X")
    assert ("repeated_row_derivation", "rows[0].derivations.DOMAIN") not in [
        (item.name, item.spec_path)
        for item in check_file(_spec(tmp_path, intermediate), orders=ORDERS)
    ]


def test_repeated_row_derivation_skips_existing_defaults(
    tmp_path: Path,
) -> None:
    defaulted = REPEATED.replace(
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n",
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n"
        "    derivation: {literal: DM}\n",
    )
    names = [
        (item.name, item.spec_path)
        for item in check_file(_spec(tmp_path, defaulted), orders=ORDERS)
    ]
    assert ("repeated_row_derivation", "rows[0].derivations.DOMAIN") not in names
    assert ("repeated_row_derivation", "rows[0].derivations.STUDYID") in names


def test_repeated_row_derivation_skips_mixed_drivers_except_literals(
    tmp_path: Path,
) -> None:
    mixed = REPEATED.replace(
        "  - id: b\n    derivations:",
        "  - id: b\n    dataset: OTHER\n    derivations:",
    ).replace(
        "input:\n  DM: input/dm.csv",
        "input:\n  DM: input/dm.csv\n  OTHER: input/other.csv",
    )
    names = [
        (item.name, item.spec_path)
        for item in check_file(_spec(tmp_path, mixed), orders=ORDERS)
    ]
    # A source read binds different records per driver, so it stays;
    # a literal is driver-independent and is still reported.
    assert ("repeated_row_derivation", "rows[0].derivations.STUDYID") not in names
    assert ("repeated_row_derivation", "rows[0].derivations.DOMAIN") in names


REDUNDANT = """\
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  ADSL: input/adsl.csv

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE]

intermediates:
  - id: ADSL1
    dataset: ADSL
    key: [STUDYID, USUBJID]
    no_match: null

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: AGE
    type: int
    label: Age
    derivation: ADSL1.AGE
"""


def test_redundant_intermediate_reads_the_dataset_directly(
    tmp_path: Path,
) -> None:
    path = _spec(tmp_path, REDUNDANT)
    findings = check_file(path, orders=ORDERS)
    assert [(item.name, item.spec_path, item.requirement) for item in findings] == [
        ("redundant_intermediate", "intermediates[0]", "REQ-1289"),
    ]
    assert findings[0].message == (
        "intermediate `ADSL1` reads exactly what the implicit join to `ADSL` "
        "reads; read `ADSL` directly"
    )
    changed, remaining = fix_file(path, orders=ORDERS)
    assert changed is False
    assert [item.name for item in remaining] == ["redundant_intermediate"]


def test_redundant_intermediate_keeps_narrowing_lookups(
    tmp_path: Path,
) -> None:
    filtered = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]\n    no_match: null",
        "    key: [STUDYID, USUBJID]\n"
        "    filter: \"ADSL.EFFFL = 'Y'\"\n    no_match: null",
    )
    assert _names(_spec(tmp_path, filtered)) == []
    custom = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]",
        "    key: [USUBJID]",
    )
    assert _names(_spec(tmp_path, custom)) == []
    required = REDUNDANT.replace("\n    no_match: null", "")
    assert "redundant_intermediate" not in _names(_spec(tmp_path, required))
    rename_only = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]\n    no_match: null", "    no_match: null"
    )
    # REQ-1248 owns the omitted-key rename; style stays silent.
    assert "redundant_intermediate" not in _names(_spec(tmp_path, rename_only))


MISSING_LABEL = """\
schema_version: "1.0"
domain: ADSL
keys: [STUDYID]
input:
  DM: input/dm.csv

output:
  path: adsl.csv
  columns: [STUDYID, AGE]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: AGE
    type: int
    derivation: DM.AGE
"""


def test_missing_output_column_labels(tmp_path: Path) -> None:
    path = _spec(tmp_path, MISSING_LABEL)
    findings = check_file(path, orders=ORDERS)
    assert [(item.name, item.spec_path, item.requirement) for item in findings] == [
        ("missing_label", "columns[1]", "REQ-1290"),
    ]
    assert findings[0].message == ("output column `AGE` has no label; declare `label:`")
    changed, remaining = fix_file(path, orders=ORDERS)
    assert changed is False
    assert [item.name for item in remaining] == ["missing_label"]


def test_missing_label_skips_internal_and_inherited_columns(
    tmp_path: Path,
) -> None:
    internal = MISSING_LABEL.replace(
        "  columns: [STUDYID, AGE]",
        "  columns: [STUDYID]",
    )
    assert _names(_spec(tmp_path, internal)) == []
    inherited = MISSING_LABEL.replace(
        'schema_version: "1.0"\ndomain: ADSL',
        'schema_version: "1.0"\nparents: base.yaml\ndomain: ADSL',
    )
    assert "missing_label" not in _names(_spec(tmp_path, inherited))


def test_authoring_suppression_with_a_reason_covers_its_entry(
    tmp_path: Path,
) -> None:
    text = REPEATED.replace(
        "  - id: a",
        "  # yamaa-style: allow repeated_row_derivation -- reviewed row scope\n"
        "  - id: a",
    )
    assert _names(_spec(tmp_path, text)) == []
