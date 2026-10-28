"""The 1.0 schema files follow the approved rename table, and their JSON Schemas
admit the converted examples.

``tools/migrate/renames.yaml`` (#3030 D58) lists every field, registry keyword
and enumerated value of the 0.5 schema with its 1.0 spelling. An entry the
converter copies or renames by key is compared mechanically: its 1.0 field
keeps the type and requirement, its default renamed with its values, and has
the table's kind. An entry the converter transforms (``reshape``, ``new``, or a
``rename`` by transform) has a reviewed reading in ``EXPECTED``; a removed
entry is absent. Every field, keyword and value ``yaml/`` holds is accounted
for by an entry, and no reserved spelling is admitted. The table's
``conditions``, ``paths`` and ``issues_columns`` describe diagnostics, which no
schema holds. Its summary states the counts the table holds.
"""

import collections
import copy
import functools
import json
import re
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest import mock

from jsonschema import Draft202012Validator

from tools.packaging import generate_json_schema as generator
from tools.paths import ROOT
from tools.tests import support
from tools.validation.repository_schema import SchemaSourceError
from tools.validation.schema_bundle import Bundle, parse_type, type_text

SCHEMAS = ROOT / "yaml"
EXAMPLES = ROOT / "docs/design/interface-1.0/examples"
ABSENT = object()


VOCABULARIES = support.load_yaml(ROOT / "yaml/vocabulary/cdisc.yaml")["vocabularies"]


# The rename table and the 1.0 schema bundle are read when a test first needs
# them, not when the module is imported; no test changes them.
@functools.cache
def rename_table():
    """The converter's rename table."""
    return support.load_yaml(ROOT / "tools/migrate/renames.yaml")


@functools.cache
def v1_bundle():
    """The 1.0 schema bundle."""
    return Bundle(SCHEMAS)


@functools.cache
def v1_fields():
    """Each 1.0 field by (owner, name)."""
    return {
        (owner, item.name): item
        for owner, items in v1_bundle().owners().items()
        for item in items
    }


@dataclass(frozen=True)
class Field:
    """A 1.0 field the entry expects; ``kind`` defaults to the entry's."""

    owner: str
    name: str
    type: str
    required: bool = False
    default: object = ABSENT
    kind: str | None = None
    values: tuple = ()


@dataclass(frozen=True)
class Like:
    """A 1.0 field declared as another entry's field is."""

    owner: str
    name: str
    entry: str


@dataclass(frozen=True)
class Absent:
    owner: str
    name: str


@dataclass(frozen=True)
class Keyword:
    registry: str
    keyword: str
    payload: str


@dataclass(frozen=True)
class NoClass:
    name: str


@dataclass(frozen=True)
class Alias:
    name: str
    type: str


@dataclass(frozen=True)
class Values:
    owner: str
    name: str
    values: tuple


def header(owner, file_type):
    return [
        Field(owner, "yamaa", "language_version", required=True, values=("1.0",)),
        Field(owner, "type", "str", required=True, values=(file_type,)),
    ]


def copied(name):
    return Like("copy_submission_class", name, f"spec.submission_column_class.{name}")


def vocabulary(name):
    """The values one vocabulary of the CDISC vocabulary file lists."""
    return tuple(entry["value"] for entry in VOCABULARIES[name]["values"])


# The reviewed 1.0 reading of every entry the converter transforms.
EXPECTED = {
    "new.project.define_version": [
        Field(
            "submission_section_class",
            "define_version",
            "str",
            values=("2.0.0", *(f"2.1.{patch}" for patch in range(12))),
        )
    ],
    "new.spec.submission.mode": [
        Field(
            "submission_dataset_class",
            "mode",
            "str",
            values=("validated", "historical"),
        )
    ],
    "new.spec.submission.artifact": [
        Field("submission_dataset_class", "artifact", "relative_href", kind="path")
    ],
    "new.spec.submission.key_sequence": [
        Field("submission_metadata_fields", "key_sequence", "int")
    ],
    "new.spec.submission.non_standard": [
        Field("submission_dataset_class", "non_standard", "bool", default=False)
    ],
    "new.spec.submission.has_no_data": [
        Field("submission_metadata_fields", "has_no_data", "bool")
    ],
    # D4: the header replaces schema_version in every file type.
    "spec.root_class.schema_version": [Absent("root_class", "schema_version")],
    "project.project_class.schema_version": [Absent("project_class", "schema_version")],
    "new.spec.yamaa": header("root_class", "spec")[:1],
    "new.spec.type": header("root_class", "spec")[1:],
    "new.project.yamaa": header("project_class", "project")[:1],
    "new.project.type": header("project_class", "project")[1:],
    "new.codelists.yamaa": header("codelist_file_class", "codelists")[:1],
    "new.codelists.type": header("codelist_file_class", "codelists")[1:],
    "new.function.yamaa": header("function_definition_class", "function")[:1],
    "new.function.type": header("function_definition_class", "function")[1:],
    # D23 requires key, but D63 exempts an intermediate that drives a row
    # template, which a class field cannot condition: the description and
    # LOOKUP-3 state the requirement, and the field stays optional.
    "spec.intermediate_class.key": [
        Field("intermediate_class", "key", "match_key", required=False)
    ],
    "spec.row_class.derivations": [
        Field("row_class", "derivations", "dict[identifier, derivation]")
    ],
    "spec.column_verifications.not_missing": [
        Keyword("column_verifications", "not_missing", "[]"),
        Values("column_class", "verifications", ("not_missing",)),
    ],
    "spec.expressions.source": [
        Keyword("expressions", "source", "source_binding_class")
    ],
    "spec.submission_column_class.origin": [
        Absent("submission_column_class", "origin"),
        Field("submission_column_class", "origins", "list[submission_origin_class]"),
    ],
    "spec.submission_value_class.origin": [
        Absent("submission_value_class", "origin"),
        Field("submission_value_class", "origins", "list[submission_origin_class]"),
    ],
    "spec.cut.right": [Absent("expressions.cut", "right")],
    "new.spec.cut.closed": [
        Field(
            "expressions.cut", "closed", "str", default="left", values=("left", "right")
        )
    ],
    "spec.baseline_flag.tie_break": [Absent("expressions.baseline_flag", "tie_break")],
    "spec.aggregate_class.derive": [
        Absent("aggregate_class", "derive"),
        Field(
            "aggregate_class", "derivations", "dict[identifier, derive_binding_class]"
        ),
    ],
    "spec.derive_binding_class.name": [Absent("derive_binding_class", "name")],
    # D8: function files and the project's per-host locks.
    "project.project_class.environments": [
        Field("project_class", "environments", "environments_class")
    ],
    "project.environments_class.python": [
        Field(
            "environments_class", "python", "host_environment_class", kind="structure"
        )
    ],
    "project.environments_class.r": [
        Field("environments_class", "r", "host_environment_class", kind="structure")
    ],
    "environment.environment_class.lock": [
        Field("host_environment_class", "lock", "project_path", required=True)
    ],
    "new.project.environments.lock": [
        Field("host_environment_class", "lock", "project_path", required=True)
    ],
    "environment.environment_class.functions": [
        Field("project_class", "functions", "list[project_path]", kind="path"),
        Field(
            "function_definition_class", "name", "identifier", True, kind="identifier"
        ),
    ],
    "new.project.functions": [
        Field("project_class", "functions", "list[project_path]")
    ],
    "new.function.name": [
        Field("function_definition_class", "name", "identifier", required=True)
    ],
    "function.function_definition_class.function": [
        Absent("function_definition_class", "function")
    ],
    "new.function.implementations": [
        Field(
            "function_definition_class",
            "implementations",
            "dict[host_language, qualified_callable]",
            required=True,
        )
    ],
    "function.function_date_literal_class.date": [
        NoClass("function_date_literal_class"),
        Alias("function_value", "literal_value"),
    ],
    "function.function_datetime_literal_class.datetime": [
        NoClass("function_datetime_literal_class"),
        Alias("function_value", "literal_value"),
    ],
    # D16 and P-STD-1.
    "project.submission_section_class.standard": [
        Absent("submission_section_class", "standard"),
        Field(
            "submission_section_class", "standards", "list[implementation_guide_class]"
        ),
    ],
    "new.spec.submission.standard": [
        Field("submission_dataset_class", "standard", "standard_reference")
    ],
    "new.project.standards.status=Provisional": [
        Values("implementation_guide_class", "status", ("Provisional",))
    ],
    # #3821: a section's transport writes Dataset-JSON 1.1 in its
    # newline-delimited representation (JSON-1, JSON-4).
    "new.project.transport=dataset_ndjson": [
        Values("submission_section_class", "transport", ("dataset_ndjson",))
    ],
    # P-VOC-1: roles and a terminology standard's name close to the lists the
    # CDISC vocabulary file holds.
    "new.spec.submission.role=closed": [
        Values("submission_role_fields", "role", vocabulary("role"))
    ],
    "new.codelists.standard.name=closed": [
        Values("terminology_standard_class", "name", vocabulary("standard_name"))
    ],
    # P-VLM-1 and D66: NE and NOTIN beside the list of values.
    "new.spec.value_level.comparator": [
        Field(
            "submission_value_class",
            "where",
            "dict[identifier, where_condition]",
            required=True,
        ),
        Field("where_comparison_class", "comparator", "str", values=("NE", "NOTIN")),
        Field("where_comparison_class", "values", "list[where_value]", required=True),
    ],
    "new.spec.model": [
        Field("root_class", "model", "str", values=("SDTM", "SEND", "ADaM"))
    ],
    "new.spec.copy": [
        Field("column_copy_class", "copy", "copy_class", required=True),
        Field("copy_class", "dataset", "identifier", True, kind="identifier"),
        Field("copy_class", "columns", "list[identifier]", True, kind="identifier"),
        Field("copy_class", "submission", "copy_submission_class", kind="structure"),
        copied("core"),
        copied("mandatory"),
        copied("role"),
    ],
    "new.spec.input.key": [Field("dataset_class", "key", "list[identifier]")],
    # D15 and D79: a csv input declares a leading byte-order mark (CSV-10).
    "new.spec.input.byte_order_mark": [
        Field("dataset_class", "byte_order_mark", "bool")
    ],
    # D208: `not_missing` selects its rows and allows a share (CHECK-5).
    "new.spec.column_verifications.not_missing.when": [
        Field("column_verifications.not_missing", "when", "predicate")
    ],
    "new.spec.column_verifications.not_missing.max_fraction": [
        Field("column_verifications.not_missing", "max_fraction", "float")
    ],
    # D210: an intermediate's `unique` may warn under keep and order_by
    # (LOOKUP-17).
    "new.spec.intermediate_unique_arguments.severity": [
        Field(
            "intermediate_unique_arguments",
            "severity",
            "verification_severity",
            default="error",
            values=("error", "warning"),
        )
    ],
    # D206: a named intermediate selects the candidate nearest a reference
    # (LOOKUP-26).
    "new.spec.intermediate.nearest": [
        Field("intermediate_class", "nearest", "nearest_class"),
        Field("nearest_class", "source", "identifier", True, kind="identifier"),
        Field("nearest_class", "to", "match_value", True, kind="expression"),
        Field(
            "nearest_class",
            "unit",
            "str",
            True,
            kind="value",
            values=("day", "week", "month", "year", "second", "minute", "hour"),
        ),
    ],
    # D263: a named intermediate pairs its dataset with a second relation
    # (LOOKUP-27).
    "new.spec.intermediate.inner_join": [
        Field("intermediate_class", "inner_join", "identifier")
    ],
    "new.spec.clear": [Field("root_class", "clear", "list[spec_path]")],
    "new.spec.include": [Field("root_class", "include", "list[project_path]")],
    # D264: str_case writes proper case as SAS PROPCASE does, after the
    # delimiters a declared list names (TEXT-5).
    "new.spec.str_case.to=proper": [Values("expressions.str_case", "to", ("proper",))],
    "new.spec.str_case.delimiters": [
        Field("expressions.str_case", "delimiters", "str")
    ],
    # D264: duration_text writes a number in a declared unit as ISO 8601
    # duration text, the inverse of duration_value (TIME-21).
    "new.spec.expressions.duration_text": [
        Keyword("expressions", "duration_text", "[]"),
        Field("expressions.duration_text", "source", "variable", True, kind="name"),
        Field(
            "expressions.duration_text",
            "unit",
            "str",
            True,
            kind="value",
            values=("second", "minute", "hour", "day"),
        ),
    ],
    # D264: the positional text verbs (TEXT-13, TEXT-14).
    "new.spec.expressions.str_substr": [
        Keyword("expressions", "str_substr", "[]"),
        Field("expressions.str_substr", "source", "variable", True, kind="name"),
        Field("expressions.str_substr", "start", "int", kind="value"),
        Field("expressions.str_substr", "length", "int", kind="value"),
        Field("expressions.str_substr", "missing", "substitute_value", kind="value"),
    ],
    # #3959: the pieces of a text broken between words, declared instead of
    # start (TEXT-16).
    "new.spec.expressions.str_substr.piece": [
        Field("expressions.str_substr", "piece", "int", kind="value")
    ],
    "new.spec.expressions.str_length": [
        Keyword("expressions", "str_length", "[]"),
        Field("expressions.str_length", "source", "variable", True, kind="name"),
        Field("expressions.str_length", "missing", "substitute_value", kind="value"),
    ],
    # D264: a number written as fixed-decimal text, as SAS put(x, w.d)
    # writes a value that fits (TEXT-15).
    "new.spec.expressions.str_decimal": [
        Keyword("expressions", "str_decimal", "[]"),
        Field("expressions.str_decimal", "source", "variable", True, kind="name"),
        Field("expressions.str_decimal", "decimals", "int", True, kind="value"),
        Field("expressions.str_decimal", "width", "int", kind="value"),
        Field("expressions.str_decimal", "missing", "substitute_value", kind="value"),
    ],
    # #3774: the fields STR_JOIN and COUNT(x) read besides their argument
    # (FORMULA-18).
    "new.spec.aggregate_class.order_by": [
        Field("aggregate_class", "order_by", "list[order_by_term]")
    ],
    "new.spec.aggregate_class.separator": [
        Field("aggregate_class", "separator", "str")
    ],
    "new.spec.aggregate_class.distinct": [Field("aggregate_class", "distinct", "bool")],
    # D264: date_text reads a declared, closed list of date forms (TIME-22).
    "new.spec.expressions.date_text": [
        Keyword("expressions", "date_text", "[]"),
        Field("expressions.date_text", "source", "variable", True, kind="name"),
        Field(
            "expressions.date_text",
            "forms",
            "list[date_form]",
            True,
            kind="value",
            values=(
                "iso",
                "ddmonyyyy",
                "mdy_slash",
                "dmy_slash",
                "yyyymmdd",
                "monyyyy",
            ),
        ),
        Field("expressions.date_text", "invalid", "substitute_value", kind="value"),
    ],
    # #3779: the probability QUANTILE reads (FORMULA-18).
    "new.spec.aggregate_class.quantile": [
        Field("aggregate_class", "quantile", "float")
    ],
    # A lint suppression comment and a profile chosen by the path's
    # extension: neither is a schema field.
    "new.any.suppression_comment": [],
    "new.spec.odm_input": [],
}
# A keep entry whose type a new entry widens.
RETYPED = {
    "spec.root_class.columns": ("list[column_entry]", "new.spec.copy"),
    "spec.submission_value_class.where": (
        "dict[identifier, where_condition]",
        "new.spec.value_level.comparator",
    ),
}
# Reshaped entries whose table kind is the 0.5 field's: the field holds
# something else in 1.0.
KIND_EXCEPTIONS = {
    # The host's path moves into {lock: PATH}; the lock keeps kind path.
    "project.environments_class.python": "structure",
    "project.environments_class.r": "structure",
    # The map of definitions becomes the project's list of function files
    # (new.project.functions); each definition is a function file's root.
    "environment.environment_class.functions": "path",
}
# Values of a removed field: the table keeps them, but no 1.0 field holds them.
REMOVED_FIELD_VALUES = {"spec.impute.minimum_source_precision"}


def location(entry):
    """The 0.5 owner and field an entry names, from its schema anchor."""
    anchor = entry["schema"].split("#", 1)[1].split(" (", 1)[0]
    owner, name = anchor.rsplit(".", 1)
    return owner, name


def normalized(text):
    return re.sub(r"\s+", "", str(text))


def renamed_fields():
    """0.5 (owner, field) to its 1.0 name, for fields renamed by key."""
    result = {}
    for entry in rename_table()["entries"]:
        convert = entry["convert"]
        if "schema" not in entry or "=" in entry["id"]:
            continue
        if convert["action"] == "rename_key":
            result[location(entry)] = convert["new_name"]
    return result


def value_renames():
    """(owner, 1.0 field) to {0.5 value: 1.0 value}."""
    result = {}
    renamed = renamed_fields()
    for entry in rename_table()["entries"]:
        if entry["kind"] == "value" and "schema" in entry:
            owner, name = location(entry)
            field = renamed.get((owner, name), name)
            result.setdefault((owner, field), {})[entry["today"]] = entry["proposed"]
    return result


def is_value(entry):
    return "=" in entry["id"]


def is_keyword(entry):
    return str(entry.get("type", "")).startswith("registry keyword")


def entry_kind(entry_id):
    entry = next(item for item in rename_table()["entries"] if item["id"] == entry_id)
    return entry.get("field_kind")


class RenameTableTests(unittest.TestCase):
    """Each rename-table entry against yaml/."""

    def setUp(self):
        self.renamed = renamed_fields()
        self.value_renames = value_renames()

    def descriptor_failures(self, label, owner, name, expected):
        """Compare one 1.0 field with what an entry expects of it."""
        field = v1_fields().get((owner, name))
        if field is None:
            return [f"{label}: {owner}.{name} is missing"]
        failures = []
        actual = type_text(field.descriptor)
        if normalized(actual) != normalized(expected["type"]):
            alias = v1_bundle().aliases.get(actual, {})
            # A named type that only gives an inline union its kind.
            same = "type" in alias and normalized(type_text(alias)) == normalized(
                expected["type"]
            )
            if not same:
                failures.append(
                    f"{label}: type {actual!r}, expected {expected['type']!r}"
                )
        if bool(field.descriptor.get("required")) != expected["required"]:
            failures.append(f"{label}: required {not expected['required']}")
        default = field.descriptor.get("default", ABSENT)
        if default != expected["default"]:
            failures.append(
                f"{label}: default {default!r}, expected {expected['default']!r}"
            )
        kind = v1_bundle().descriptor_kind(field.descriptor, label)
        if expected["kind"] is not None and kind != expected["kind"]:
            failures.append(f"{label}: kind {kind}, expected {expected['kind']}")
        if expected.get("values"):
            spelled = v1_bundle().values(field.descriptor) or ()
            missing = [value for value in expected["values"] if value not in spelled]
            if missing:
                failures.append(f"{label}: values lack {missing}")
        return failures

    def test_every_entry_is_read_exactly_once(self):
        transformed = {
            entry["id"]
            for entry in rename_table()["entries"]
            if entry["kind"] in {"reshape", "new"}
            or (entry["kind"] == "rename" and entry["convert"]["action"] == "transform")
        }
        self.assertEqual(set(EXPECTED), transformed)
        for entry_id, (_, justified_by) in RETYPED.items():
            self.assertIn(justified_by, EXPECTED, entry_id)

    def test_the_summary_states_the_tables_counts(self):
        # Every count the summary states. Two branches that each add an
        # entry and make the same change to a count merge without a
        # conflict, and the count then falls short by one.
        table = rename_table()
        entries = table["entries"]
        by_kind_and_decision = collections.Counter(
            f"{entry['kind']}, {entry['decision'].split(' (', 1)[0]}"
            for entry in entries
        )
        self.assertEqual(
            table["summary"],
            {
                "entries": len(entries),
                "changes": sum(entry["kind"] != "keep" for entry in entries),
                "by_kind_and_decision": dict(by_kind_and_decision),
                "conditions": len(table["conditions"]),
                "paths": len(table["paths"]),
                "reserved": len(table["reserved"]),
            },
            "the summary",
        )

    def test_copied_and_renamed_fields_keep_type_requirement_default_and_kind(self):
        failures = []
        for entry in rename_table()["entries"]:
            if is_value(entry) or is_keyword(entry) or entry["id"] in EXPECTED:
                continue
            if entry["kind"] not in {"keep", "rename"}:
                continue
            owner, name = location(entry)
            new_name = self.renamed.get((owner, name), name)
            renames = self.value_renames.get((owner, new_name), {})
            default = entry.get("default", ABSENT)
            if isinstance(default, str):
                default = renames.get(default, default)
            expected = {
                "type": RETYPED.get(entry["id"], (entry["type"],))[0],
                "required": bool(entry.get("required")),
                "default": default,
                "kind": entry["field_kind"],
            }
            failures += self.descriptor_failures(entry["id"], owner, new_name, expected)
            if new_name != name and (owner, name) in v1_fields():
                failures.append(f"{entry['id']}: the 0.5 spelling {name} remains")
        self.assertEqual(failures, [])

    def test_removed_entries_are_absent(self):
        failures = []
        for entry in rename_table()["entries"]:
            if entry["kind"] != "removed":
                continue
            owner, name = location(entry)
            if is_keyword(entry):
                if name in v1_bundle().registries.get(owner, {}):
                    failures.append(f"{entry['id']}: keyword {name} remains")
            elif (owner, name) in v1_fields():
                failures.append(f"{entry['id']}: field {owner}.{name} remains")
        # The classes that held only removed spellings are gone too.
        for name in (
            "environment_class",
            "intermediate_expression_class",
            "function_string_literal_class",
            "function_date_literal_class",
            "function_datetime_literal_class",
        ):
            if name in v1_bundle().classes:
                failures.append(f"class {name} remains")
        self.assertEqual(failures, [])

    def test_kept_registry_keywords_remain(self):
        failures = []
        for entry in rename_table()["entries"]:
            if is_keyword(entry) and entry["kind"] == "keep":
                registry, keyword = location(entry)
                if keyword not in v1_bundle().registries.get(registry, {}):
                    failures.append(f"{entry['id']}: keyword {keyword} is missing")
        self.assertEqual(failures, [])

    def test_enumerated_values_take_their_1_0_spelling(self):
        failures = []
        for entry in rename_table()["entries"]:
            if not is_value(entry) or "schema" not in entry:
                continue
            owner, name = location(entry)
            field_entry = entry["id"].split("=", 1)[0]
            if field_entry in REMOVED_FIELD_VALUES:
                self.assertNotIn((owner, name), v1_fields())
                continue
            field = entry["convert"].get("field") or self.renamed.get(
                (owner, name), name
            )
            spelled = v1_bundle().values(v1_fields()[(owner, field)].descriptor) or ()
            value = entry["proposed"] if entry["kind"] == "value" else entry["today"]
            if value not in spelled:
                failures.append(f"{entry['id']}: {owner}.{field} lacks {value!r}")
            if entry["kind"] == "value" and entry["today"] != value:
                if entry["today"] in spelled:
                    failures.append(f"{entry['id']}: {entry['today']!r} remains")
        self.assertEqual(failures, [])

    def test_transformed_entries_match_their_reviewed_reading(self):
        failures = []
        for entry_id, checks in EXPECTED.items():
            table_kind = entry_kind(entry_id)
            fields = [check for check in checks if isinstance(check, (Field, Like))]
            for check in checks:
                primary = bool(fields) and check is fields[0]
                failures += self.check(entry_id, table_kind, check, primary)
        self.assertEqual(failures, [])

    def check(self, entry_id, table_kind, check, primary=False):
        if isinstance(check, Field):
            # The table's kind is the kind of the entry's own field; the
            # fields of a class it introduces state theirs.
            kind = check.kind or table_kind
            if primary and check.kind and table_kind and check.kind != table_kind:
                if KIND_EXCEPTIONS.get(entry_id) != check.kind:
                    return [f"{entry_id}: kind {check.kind} departs from the table's"]
            expected = {
                "type": check.type,
                "required": check.required,
                "default": check.default,
                "kind": kind,
                "values": check.values,
            }
            return self.descriptor_failures(entry_id, check.owner, check.name, expected)
        if isinstance(check, Like):
            source = next(
                item for item in rename_table()["entries"] if item["id"] == check.entry
            )
            expected = {
                "type": source["type"],
                "required": bool(source.get("required")),
                "default": source.get("default", ABSENT),
                "kind": source["field_kind"],
            }
            failures = self.descriptor_failures(
                entry_id, check.owner, check.name, expected
            )
            other = v1_fields().get(location(source))
            mine = v1_fields().get((check.owner, check.name))
            if (
                other
                and mine
                and v1_bundle().values(other.descriptor)
                != v1_bundle().values(mine.descriptor)
            ):
                failures.append(f"{entry_id}: {check.owner}.{check.name} values differ")
            return failures
        if isinstance(check, Absent):
            if (check.owner, check.name) in v1_fields():
                return [f"{entry_id}: {check.owner}.{check.name} remains"]
            return []
        if isinstance(check, Keyword):
            payload = v1_bundle().registries.get(check.registry, {}).get(check.keyword)
            if payload is None:
                return [f"{entry_id}: keyword {check.keyword} is missing"]
            shape = "[]" if isinstance(payload, list) else type_text(payload)
            if shape != check.payload:
                return [f"{entry_id}: {check.keyword} payload {shape}"]
            return []
        if isinstance(check, NoClass):
            return [f"{entry_id}: class {check.name} remains"] * (
                check.name in v1_bundle().classes
            )
        if isinstance(check, Alias):
            alias = v1_bundle().aliases.get(check.name, {})
            if alias.get("type") != check.type:
                return [f"{entry_id}: {check.name} is {alias.get('type')!r}"]
            return []
        field = v1_fields().get((check.owner, check.name))
        spelled = v1_bundle().values(field.descriptor) if field else ()
        missing = [value for value in check.values if value not in (spelled or ())]
        return [f"{entry_id}: values lack {missing}"] if missing else []

    def accounted(self):
        """(owner, field) pairs and values the table accounts for."""
        fields = set()
        values = {}
        for entry in rename_table()["entries"]:
            if entry["id"] in EXPECTED or "schema" not in entry or is_keyword(entry):
                continue
            owner, name = location(entry)
            if is_value(entry):
                field = entry["convert"].get("field") or self.renamed.get(
                    (owner, name), name
                )
                value = (
                    entry["proposed"] if entry["kind"] == "value" else entry["today"]
                )
                values.setdefault((owner, field), set()).add(value)
            elif entry["kind"] in {"keep", "rename"}:
                fields.add((owner, self.renamed.get((owner, name), name)))
        for checks in EXPECTED.values():
            for check in checks:
                if isinstance(check, (Field, Like)):
                    fields.add((check.owner, check.name))
                if isinstance(check, (Field, Values)):
                    values.setdefault((check.owner, check.name), set()).update(
                        check.values
                    )
                if isinstance(check, Like):
                    source = location(
                        next(
                            item
                            for item in rename_table()["entries"]
                            if item["id"] == check.entry
                        )
                    )
                    values.setdefault((check.owner, check.name), set()).update(
                        values.get(source, ())
                    )
        return fields, values

    def test_no_field_keyword_or_value_the_table_does_not_list(self):
        fields, values = self.accounted()
        # A kept or reshaped 0.5 keyword, or one a new entry's reading names.
        keywords = {
            location(entry)
            for entry in rename_table()["entries"]
            if is_keyword(entry) and entry["kind"] in {"keep", "reshape"}
        } | {
            (check.registry, check.keyword)
            for checks in EXPECTED.values()
            for check in checks
            if isinstance(check, Keyword)
        }
        failures = []
        for file_type, (entry, root) in generator.FILE_TYPES.items():
            bundle = Bundle(SCHEMAS, entry)
            for owner in bundle.reachable(root):
                for item in v1_bundle().owners()[owner]:
                    if (owner, item.name) not in fields and (
                        item.origin,
                        item.name,
                    ) not in fields:
                        failures.append(f"{file_type}: {owner}.{item.name} is unlisted")
                    listed = values.get((owner, item.name), set()) | values.get(
                        (item.origin, item.name), set()
                    )
                    for value in v1_bundle().values(item.descriptor) or ():
                        if value not in listed:
                            failures.append(
                                f"{file_type}: {owner}.{item.name} value {value!r} is unlisted"
                            )
        for registry, entries in v1_bundle().registries.items():
            for keyword in entries:
                if (registry, keyword) not in keywords:
                    failures.append(f"keyword {registry}.{keyword} is unlisted")
        self.assertEqual(sorted(set(failures)), [])


def example_files():
    """Each converted example file, its file type, and whether it is complete:
    a layer that names a parent or fragments, or that another file names as
    its parent or a fragment, is one layer of a composition."""
    paths = sorted(EXAMPLES.rglob("*.yaml"))
    documents = {path: support.load_yaml(path) for path in paths}
    named = {
        (path.parent / layer).resolve()
        for path, document in documents.items()
        for layer in [
            *([document["parent"]] if "parent" in document else []),
            *document.get("include", []),
        ]
    }
    for path, document in documents.items():
        complete = (
            "parent" not in document
            and "include" not in document
            and path.resolve() not in named
        )
        yield path, document, complete


def validator(file_type, start=None):
    schema = json.loads(generator.target(file_type).read_text(encoding="ascii"))
    if start is not None:
        schema = {**schema, "$ref": f"#/$defs/{start}"}
    return Draft202012Validator(schema)


def messages(validator_, document):
    return [
        f"{'/'.join(map(str, error.absolute_path))}: {error.message}"
        for error in validator_.iter_errors(document)
    ]


class JsonSchemaTests(unittest.TestCase):
    def test_define_versions_admit_every_published_release_since_2_0_0(self):
        section = validator("project", "submission_section_class")
        cases = [
            *(
                (version, True)
                for version in ["2.0.0", *(f"2.1.{patch}" for patch in range(12))]
            ),
            *(
                (version, False)
                for version in [
                    "1.0.0",
                    "2.0",
                    "2.0.1",
                    "2.1",
                    "2.1.01",
                    "2.1.12",
                    "2.2.0",
                    "3.0.0",
                    None,
                    2.1,
                ]
            ),
        ]
        for version, admitted in cases:
            with self.subTest(version=version):
                self.assertEqual(
                    section.is_valid(
                        {"specs": ["spec.yaml"], "define_version": version}
                    ),
                    admitted,
                )

    def test_committed_json_schemas_are_current_and_valid(self):
        self.assertEqual(
            sorted(path.name for path in generator.TARGET.iterdir()),
            sorted(generator.target(name).name for name in generator.FILE_TYPES),
        )
        for file_type in generator.FILE_TYPES:
            with self.subTest(file_type=file_type):
                path = generator.target(file_type)
                self.assertEqual(
                    path.read_bytes(), generator.render(file_type).encode("ascii")
                )
                Draft202012Validator.check_schema(json.loads(path.read_text("ascii")))

    def test_the_check_refuses_a_stale_missing_or_extra_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "json"
            with (
                mock.patch.object(generator, "TARGET", target),
                mock.patch.object(generator, "ROOT", Path(directory)),
            ):

                def run(*arguments):
                    with mock.patch("builtins.print"):
                        return generator.main(list(arguments))

                self.assertEqual(run(), 1)
                self.assertEqual(run("--write"), 0)
                self.assertEqual(run(), 0)
                spec = target / "spec.schema.json"
                spec.write_bytes(spec.read_bytes().replace(b'"spec"', b'"spek"', 1))
                self.assertEqual(run(), 1)
                self.assertEqual(run("--write"), 0)
                (target / "environment.schema.json").write_text("{}\n")
                self.assertEqual(run(), 1)

    def test_every_field_carries_its_kind_and_notation_type(self):
        for file_type in generator.FILE_TYPES:
            schema = json.loads(generator.target(file_type).read_text("ascii"))
            stack = list(schema["$defs"].values())
            while stack:
                node = stack.pop()
                if isinstance(node, dict):
                    for name, value in node.get("properties", {}).items():
                        if node.get("minProperties") == 1:
                            continue  # A registry's keywords are not fields.
                        self.assertIn("x-yamaa-kind", value, name)
                        self.assertIn("x-yamaa-type", value, name)
                    stack.extend(node.values())
                elif isinstance(node, list):
                    stack.extend(node)

    def test_the_six_converted_examples_validate(self):
        seen = set()
        for path, document, complete in example_files():
            seen.add(path.relative_to(EXAMPLES).parts[0])
            with self.subTest(example=path.relative_to(EXAMPLES).as_posix()):
                file_type = document["type"]
                self.assertEqual(messages(validator(file_type), document), [])
                if file_type == "spec" and complete:
                    complete_spec = validator("spec", "root_class")
                    self.assertEqual(messages(complete_spec, document), [])
        self.assertEqual(
            seen,
            {
                "adam-adsl-bmi",
                "adam-adtte-pfs",
                "schema-inheritance",
                "schema-inheritance-clear",
                "sdtm-dm-metadata",
                "sdtm-lb-multiform",
            },
        )

    def test_a_layer_defers_requiredness_only_inside_keyed_members(self):
        layer = validator("spec")
        complete = validator("spec", "root_class")
        organization = support.load_yaml(
            EXAMPLES / "schema-inheritance/spec_organization.yaml"
        )
        self.assertEqual(messages(layer, organization), [])
        self.assertIn(
            ": 'domain' is a required property", messages(complete, organization)
        )
        partial = {"yamaa": "1.0", "type": "spec", "columns": [{"name": "AVAL"}]}
        self.assertEqual(messages(layer, partial), [])
        for document, expected in (
            # A whole root field is complete in the layer that writes it.
            ({**partial, "output": {"path": "a.csv"}}, "'columns' is a required"),
            # Every layer carries the header.
            ({"type": "spec"}, "'yamaa' is a required"),
            # A keyed member carries its identity.
            ({**partial, "columns": [{"label": "Value"}]}, "is not valid under any"),
        ):
            with self.subTest(document=document):
                self.assertTrue(
                    any(expected in message for message in messages(layer, document)),
                    messages(layer, document),
                )

    def test_0_5_spellings_are_refused(self):
        spec = support.load_yaml(EXAMPLES / "adam-adtte-pfs/spec.yaml")
        layer = validator("spec")
        aval = next(
            index
            for index, column in enumerate(spec["columns"])
            if column["name"] == "AVAL"
        )
        for mutate in (
            lambda d: d.update(schema_version="1.0"),
            lambda d: d["columns"][0].update(derivation={"literal": "X"}),
            lambda d: d["columns"][0].update(derivation={"compute": {"expr": "1 + 1"}}),
            lambda d: d["columns"][0].update(derivation={"source": "ADSL.STUDYID"}),
            lambda d: d["columns"][aval]["derivation"]["diff"].update(
                bounds="inclusive"
            ),
            lambda d: d["intermediates"][0].update(no_match=None),
            lambda d: d["columns"][0].update(
                submission={"origin": {"type": "Assigned"}}
            ),
        ):
            document = copy.deepcopy(spec)
            mutate(document)
            self.assertNotEqual(messages(layer, document), [])
        project = support.load_yaml(EXAMPLES / "sdtm-dm-metadata/project.yaml")
        for mutate in (
            lambda d: d["sdtm"].update(datasets="json"),
            lambda d: d["sdtm"].update(
                standard={"name": "SDTMIG", "version": "3.4", "status": "Final"}
            ),
            lambda d: d.update(environments={"python": "python/environment.yaml"}),
        ):
            document = copy.deepcopy(project)
            mutate(document)
            self.assertNotEqual(messages(validator("project"), document), [])


def value_level(document):
    """A spec whose first column carries one valid value-level entry."""
    column = document["columns"][0]["submission"]
    column["value_level"] = [{"id": "VL1", "where": {"DOMAIN": ["DM"]}}]
    return column["value_level"][0]


def origin_documents(document):
    return document["columns"][3]["submission"]["origins"][0]["documents"][0]


# Each reserved spelling of the table, written where it would sit: the 1.0
# schema has no such field or value, and the JSON Schema refuses it.
RESERVED = {
    ("{comparator: LT | LE | GT | GE, values: [V]}", "a value-level where column"): (
        "spec",
        [("where_comparison_class", "comparator")],
        lambda d: value_level(d)["where"].update(
            DOMAIN={"comparator": "LT", "values": ["DM"]}
        ),
    ),
    ("where_any: [{COLUMN: [V, ...]}, ...]", "a value-level entry, instead of where"): (
        "spec",
        [("submission_value_class", "where_any")],
        lambda d: value_level(d).update(where_any=[{"DOMAIN": ["DM"]}]),
    ),
    ("qualified where columns, such as DM.COUNTRY", "a value-level where"): (
        "spec",
        [],
        lambda d: value_level(d)["where"].update({"DM.COUNTRY": ["USA"]}),
    ),
    (
        "methods: [{id, name, type, description, expressions, documents}]",
        "a project section",
    ): (
        "project",
        [("submission_section_class", "methods")],
        lambda d: d["sdtm"].update(methods=[{"id": "M1", "description": "Text."}]),
    ),
    ("method: {id: NAME}", "column and value-level submission"): (
        "spec",
        [("submission_method_class", "id")],
        lambda d: d["columns"][2]["submission"].update(method={"id": "M1"}),
    ),
    ("comments: [{id, text, documents}]", "a project section"): (
        "project",
        [("submission_section_class", "comments")],
        lambda d: d["sdtm"].update(comments=[{"id": "C1", "text": "Text."}]),
    ),
    ("comment: {id: NAME}", "dataset, column and value-level submission"): (
        "spec",
        [("submission_comment_class", "id")],
        lambda d: d["submission"].update(comment={"id": "C1"}),
    ),
    ("columns: [{name, type, label, submission}]", "a project section"): (
        "project",
        [("submission_section_class", "columns")],
        lambda d: d["sdtm"].update(columns=[{"name": "STUDYID", "type": "str"}]),
    ),
    ("use: NAME", "a column entry, instead of name"): (
        "spec",
        [("column_class", "use"), ("column_copy_class", "use")],
        lambda d: d["columns"].append({"use": "STUDYID"}),
    ),
    ("submission: {non_standard: true}", "a column"): (
        "spec",
        [("submission_column_class", "non_standard")],
        lambda d: d["columns"][0]["submission"].update(non_standard=True),
    ),
    ("submission: {evaluator: TEXT}", "a column"): (
        "spec",
        [("submission_column_class", "evaluator")],
        lambda d: d["columns"][0]["submission"].update(evaluator="INVESTIGATOR"),
    ),
    ("submission: {supplemental: {dataset: NAME, idvar: COLUMN}}", "a dataset"): (
        "spec",
        [("submission_dataset_class", "supplemental")],
        lambda d: d["submission"].update(supplemental={"dataset": "SUPPDM"}),
    ),
    ("submission: {split: {by: COLUMN, datasets: {NAME: [V, ...]}}}", "a dataset"): (
        "spec",
        [("submission_dataset_class", "split")],
        lambda d: d["submission"].update(
            split={"by": "SEX", "datasets": {"DMF": ["F"]}}
        ),
    ),
    ("pages: {type, first, last}", "a document reference"): (
        "spec",
        [("page_reference_class", "first"), ("page_reference_class", "last")],
        lambda d: origin_documents(d)["pages"].update(first=1, last=2),
    ),
    ("expressions: [{context, code}, ...]", "a method, instead of expression"): (
        "spec",
        [("submission_method_class", "expressions")],
        lambda d: d["columns"][2]["submission"].update(
            method={
                "description": "Text.",
                "expressions": [{"context": "R", "code": "x"}],
            }
        ),
    ),
    ("external: {ref: PATH}", "an external codelist"): (
        "project",
        [("external_codelist_class", "ref")],
        lambda d: d["codelists"][0]["codelists"][3]["external"].update(
            ref="iso3166.xml"
        ),
    ),
    ("submission: {standard: {name: NAME, version: TEXT}}", "a dataset"): (
        "spec",
        [],
        lambda d: d["submission"].update(standard={"name": "SDTMIG", "version": "3.4"}),
    ),
    ("transactions: replay", "an ODM input"): (
        "spec",
        [("dataset_class", "transactions")],
        lambda d: d["input"].update(
            DM_RAW={"path": "odm.xml", "transactions": "replay"}
        ),
    ),
    ("archive_member: NAME", "an ODM input"): (
        "spec",
        [("dataset_class", "archive_member")],
        lambda d: d["input"].update(DM_RAW={"path": "odm.xml", "archive_member": "a"}),
    ),
    ("odm: {item: D.I, scope: subject | event | form | item_group}", "an odm read"): (
        "spec",
        [("odm_class", "scope")],
        lambda d: d["columns"][3].update(
            derivation={"odm": {"item": "DM_RAW.IT.SUBJID", "scope": "subject"}}
        ),
    ),
}
RESERVED_VALUES = {("where_comparison_class", "comparator"): ("LT", "LE", "GT", "GE")}


class ReservedSpellingTests(unittest.TestCase):
    def setUp(self):
        self.documents = {
            "spec": support.load_yaml(EXAMPLES / "sdtm-dm-metadata/spec.yaml"),
            "project": support.load_yaml(EXAMPLES / "sdtm-dm-metadata/project.yaml"),
        }

    def test_every_reserved_spelling_has_a_check(self):
        listed = {
            (item["spelling"], item["where"]) for item in rename_table()["reserved"]
        }
        self.assertEqual(set(RESERVED), listed)

    def test_no_reserved_spelling_is_admitted(self):
        for (spelling, where), (file_type, fields, write) in RESERVED.items():
            with self.subTest(spelling=spelling, where=where):
                for owner, name in fields:
                    if (owner, name) in RESERVED_VALUES:
                        spelled = v1_bundle().values(
                            v1_fields()[(owner, name)].descriptor
                        )
                        for value in RESERVED_VALUES[(owner, name)]:
                            self.assertNotIn(value, spelled)
                    else:
                        self.assertNotIn((owner, name), v1_fields())
                document = copy.deepcopy(self.documents[file_type])
                write(document)
                self.assertNotEqual(messages(validator(file_type), document), [])

    def test_the_documents_the_checks_write_into_are_valid(self):
        spec = copy.deepcopy(self.documents["spec"])
        value_level(spec)
        self.assertEqual(messages(validator("spec"), spec), [])
        self.assertEqual(messages(validator("project"), self.documents["project"]), [])
        # NE and NOTIN are admitted in 1.0 (D66), beside the plain list.
        for comparator, values in (("NE", ["DM"]), ("NOTIN", ["DM", "AE"])):
            with self.subTest(comparator=comparator):
                document = copy.deepcopy(self.documents["spec"])
                value_level(document)["where"]["DOMAIN"] = {
                    "comparator": comparator,
                    "values": values,
                }
                self.assertEqual(messages(validator("spec"), document), [])


class SchemaNotationTests(unittest.TestCase):
    """The kind rules of the notation extension (yaml/README.md)."""

    def bundle(self, text):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / "schema_test.yaml"
        path.write_text('version: "1.0"\n' + text, encoding="ascii")
        return Bundle(path.parent, path.name)

    def test_a_field_takes_the_kind_of_its_type(self):
        bundle = self.bundle(
            "example_class:\n"
            "    - name: {type: identifier}\n"
            '    - names: {type: "list[identifier]"}\n'
            '    - labels: {type: "dict[str, str]"}\n'
            "    - nested: {type: example_class}\n"
            "    - shorthand: {type: shorthand}\n"
            "identifier:\n    type: str\n    kind: identifier\n"
            "shorthand:\n    type: [identifier, example_class]\n    kind: name\n"
        )
        kinds = {
            item.name: bundle.descriptor_kind(item.descriptor, item.name)
            for item in bundle.fields("example_class")
        }
        self.assertEqual(
            kinds,
            {
                "name": "identifier",
                "names": "identifier",
                "labels": "value",
                "nested": "structure",
                "shorthand": "name",
            },
        )

    def test_a_union_of_different_kinds_is_a_named_type_that_declares_one(self):
        bundle = self.bundle(
            "example_class:\n    - mixed: {type: [identifier, int]}\n"
            "identifier:\n    type: str\n    kind: identifier\n"
        )
        (item,) = bundle.fields("example_class")
        with self.assertRaisesRegex(SchemaSourceError, "declares its kind"):
            bundle.descriptor_kind(item.descriptor, "example_class.mixed")

    def test_only_a_named_value_type_declares_a_known_kind(self):
        with self.assertRaisesRegex(SchemaSourceError, "kind must be one of"):
            self.bundle("identifier:\n    type: str\n    kind: label\n")
        bundle = self.bundle(
            "example_class:\n    - name: {type: str, kind: identifier}\n"
        )
        with self.assertRaisesRegex(
            SchemaSourceError, "named value type declares kind"
        ):
            bundle.fields("example_class")

    def test_a_class_field_declares_the_version_that_introduced_it(self):
        bundle = self.bundle(
            "example_class:\n"
            "    - name: {type: str}\n"
            '    - added: {type: str, since: "1.1"}\n'
        )
        name, added = bundle.fields("example_class")
        self.assertNotIn("since", name.descriptor)
        self.assertEqual(added.descriptor["since"], "1.1")
        properties = generator.Renderer(bundle).properties(
            "example_class", bundle.fields("example_class"), lambda item: False
        )
        self.assertNotIn("x-yamaa-since", properties["name"])
        self.assertEqual(properties["added"]["x-yamaa-since"], "1.1")
        for since in ("1.1", '"1.01"', '"2.0"', '"1."'):
            bundle = self.bundle(
                f"example_class:\n    - added: {{type: str, since: {since}}}\n"
            )
            with (
                self.subTest(since=since),
                self.assertRaisesRegex(SchemaSourceError, "since must be"),
            ):
                bundle.fields("example_class")
        with self.assertRaisesRegex(SchemaSourceError, "unknown keyword since"):
            self.bundle('identifier:\n    type: str\n    since: "1.1"\n')

    def test_type_expressions_parse_as_the_notation_writes_them(self):
        self.assertEqual(
            str(parse_type("dict[ identifier , list[str] ]")),
            "dict[identifier, list[str]]",
        )
        for text in ("list[", "dict[str]", "list[str]]", "a b"):
            with self.subTest(text=text), self.assertRaises(SchemaSourceError):
                parse_type(text)


if __name__ == "__main__":
    unittest.main()
