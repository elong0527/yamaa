"""Check that the engine reproduces the expression language's evaluation set.

`yaml/conformance/expressions.yaml` is authored truth (#3690): each case reads
one text of the closed grammar in one context, over a small table, and gives
either the value of each row or group or the one issue its run reports. The
check reads the document as YAML 1.1 and as YAML 1.2, which must agree, checks
its structure and that each case's text writes every function, reducer,
operator and literal keyword its `covers` names, and checks that its cases
cover every function and reducer of the grammar's closed lists, every
operator and literal keyword its productions write, the three truth values of
NOT, AND and OR, and the refusals of FORMULA-13 and FORMULA-27 in each
context. It then runs every
case through the installed engine, the development package `make setup`
builds, as a specification that reads the case's table and derives one
column from its text, an aggregate case with the reducer fields it gives
(`order_by`, `separator`, `distinct`, `quantile`), and compares each value
exactly, or the issue's phase, condition and rule. Nothing here decides an
outcome: every expectation stays in the document.
"""

import argparse
import csv
import datetime as dt
import json
import math
import re
import struct
import sys
import tempfile
from pathlib import Path

import yaml12

import yaml
from tools.paths import ROOT
from tools.validation.editorial import UniqueKeyLoader

DOCUMENT = Path("yaml/conformance/expressions.yaml")
GRAMMAR = Path("yaml/grammar/expression.yaml")
CONDITIONS = Path("yaml/conditions.yaml")
RULES = Path("rules")

DOCUMENT_KEYS = ("schema_version", "contract", "contract_version", "grammar", "cases")
CASE_KEYS = {
    "id",
    "context",
    "covers",
    "text",
    "columns",
    "rows",
    "groups",
    "group_by",
    "order_by",
    "separator",
    "distinct",
    "quantile",
    "type",
    "values",
    "phase",
    "condition",
    "rule",
}
# The fields of aggregate.expr's aggregate that a reducer reads besides its
# argument (FORMULA-18), which an aggregate case may give.
REDUCER_FIELDS = ("order_by", "separator", "distinct", "quantile")
ORDER_TERM_KEYS = {"variable", "direction", "nulls"}
ISSUE_KEYS = ("phase", "condition", "rule")
CONTEXTS = ("predicate", "derivation", "aggregate")
# The column types of VALUE-1, and the truth values of FORMULA-4 that a
# predicate case's values take.
TYPES = ("str", "int", "float", "date", "datetime")
TRUTH = ("TRUE", "FALSE", "UNKNOWN")
CASE_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
COLUMN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
RULE_ID = re.compile(r"^[A-Z]+-[0-9]+$")
DATE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
DATETIME = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$")
INT_RANGE = range(-(2**63), 2**63)
# The relation an aggregate case reduces, and the names the specification that
# runs a case gives its own inputs and columns, which no case column may take.
RELATION = "R"
HARNESS_NAMES = {"ROWID", "GROUPID", "RESULT", "SRC", "GROUPS", RELATION}
# The operators and literal keywords each production of the grammar writes,
# by the names `covers` gives them. The comparison operators are read from the
# `compare` production itself.
OPERATORS = {
    "disjunction": ("OR",),
    "conjunction": ("AND",),
    "negation": ("NOT",),
    "membership": ("IN", "NOT IN"),
    "range": ("BETWEEN", "NOT BETWEEN"),
    "pattern": ("LIKE", "NOT LIKE", "ESCAPE"),
    "null_test": ("IS NULL", "IS NOT NULL"),
    "sum": ("+", "-"),
    "product": ("*", "/"),
    "factor": ("unary +", "unary -"),
    "literal": ("TRUE", "FALSE", "NULL"),
    "temporal": ("DATE", "DATETIME"),
}
# What else a case may say it covers.
TOPICS = {
    "arity",
    "association",
    "context",
    "declared-order",
    "distinct",
    "exactness",
    "keyword-case",
    "literal",
    "missing",
    "non-finite",
    "overflow",
    "precedence",
    "precision",
    "record-order",
    "temporal",
    "text-order",
    "three-valued-logic",
    "typing",
    "vocabulary",
}
# The operators whose predicate cases must show TRUE, FALSE and UNKNOWN.
LOGIC = ("NOT", "AND", "OR")
# The refusals each context must show: a call its field does not admit
# (FORMULA-13) and a construct it does not admit (FORMULA-27).
REFUSALS = (
    ("predicate", "prohibited_function", "FORMULA-13"),
    ("derivation", "prohibited_function", "FORMULA-13"),
    ("derivation", "prohibited_construct", "FORMULA-27"),
    ("aggregate", "prohibited_function", "FORMULA-13"),
    ("aggregate", "prohibited_construct", "FORMULA-27"),
)
# A text's tokens, as the grammar's `string`, `number` and `name` productions
# and its operator terminals spell them. Any other character is a symbol of
# its own, which writes nothing a case covers.
TOKEN = re.compile(
    r"\s+"
    r"|(?P<string>'(?:[^']|'')*')"
    r"|(?P<number>[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)"
    r"|(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    r"|(?P<symbol><=|>=|<>|\S)"
)
# The reserved words that write an operator, so that a sign after one is not
# the binary operator. The other reserved words, NULL, TRUE and FALSE, end an
# operand, as an identifier does.
OPERATOR_WORDS = {"AND", "BETWEEN", "IN", "IS", "LIKE", "NOT", "OR"}


def load(root=ROOT):
    """The document as YAML 1.2 reads it, and the errors found reading it."""
    text = (root / DOCUMENT).read_text(encoding="utf-8")
    if not text.isascii():
        return None, [f"{DOCUMENT}: write each non-ASCII scalar as an escape"]
    try:
        document = yaml12.parse_yaml(text)
        older = yaml.load(text, Loader=UniqueKeyLoader)
    except Exception as error:
        return None, [f"{DOCUMENT}: {error}"]
    if older != document:
        return None, [
            f"{DOCUMENT}: YAML 1.1 reads a value differently from YAML 1.2; quote "
            "text, and write every float with a decimal point and any exponent "
            "signed"
        ]
    return document, []


def grammar_names(grammar):
    """The function, reducer, operator and literal names the grammar defines."""
    productions = {item["name"]: item["definition"] for item in grammar["productions"]}
    operators = re.findall(r'"([^"]*)"', productions["compare"])
    for production, names in OPERATORS.items():
        if production not in productions:
            raise ValueError(f"{GRAMMAR}: no production {production!r}")
        operators.extend(names)
    vocabulary = grammar["vocabulary"]
    return list(vocabulary["function"]), list(vocabulary["reducer"]), operators


def tokens(text):
    """A text's tokens as (kind, spelling) pairs, each name in upper case,
    since a function, reducer or keyword is spelled in any case."""
    found = []
    for match in TOKEN.finditer(text):
        kind = match.lastgroup
        if kind is not None:
            spelling = match.group(kind)
            found.append((kind, spelling.upper() if kind == "name" else spelling))
    return found


def written(text):
    """The functions, reducers, operators and literal keywords a text writes,
    by the names `covers` gives them.

    The grammar decides which construct a token belongs to: a string's
    content writes nothing; a NOT before IN, BETWEEN or LIKE, or after IS,
    belongs to that operator, and the AND after a BETWEEN to the range; a sign
    after an operand is the binary operator, and otherwise belongs to the
    number it precedes or is a unary sign; the star of `COUNT(R.*)` multiplies
    nothing; and DATE, DATETIME and ESCAPE, which are not reserved, write
    themselves only before a string. A name before a parenthesis is a call.
    """
    found = tokens(text)
    nothing = ("", "")
    names = set()
    ranges = 0
    for index, (kind, spelling) in enumerate(found):
        before = found[index - 1] if index else nothing
        after = found[index + 1] if index + 1 < len(found) else nothing
        if kind == "name":
            if spelling in ("IN", "BETWEEN", "LIKE"):
                negated = before == ("name", "NOT")
                names.add(f"NOT {spelling}" if negated else spelling)
                ranges += spelling == "BETWEEN"
            elif spelling == "NOT":
                compound = after in {
                    ("name", word) for word in ("IN", "BETWEEN", "LIKE")
                }
                if not compound and before != ("name", "IS"):
                    names.add("NOT")
            elif spelling == "NULL":
                if before == ("name", "IS"):
                    names.add("IS NULL")
                elif before == ("name", "NOT") and found[index - 2 : index - 1] == [
                    ("name", "IS")
                ]:
                    names.add("IS NOT NULL")
                else:
                    names.add("NULL")
            elif spelling == "AND":
                if ranges:
                    ranges -= 1
                else:
                    names.add("AND")
            elif spelling in ("OR", "TRUE", "FALSE"):
                names.add(spelling)
            elif spelling in ("DATE", "DATETIME", "ESCAPE"):
                if after[0] == "string":
                    names.add(spelling)
            elif after == ("symbol", "("):
                names.add(spelling)
        elif spelling in ("+", "-"):
            operand = (
                before[0] in ("number", "string")
                or before == ("symbol", ")")
                or (before[0] == "name" and before[1] not in OPERATOR_WORDS)
            )
            if operand:
                names.add(spelling)
            elif after[0] != "number":
                names.add(f"unary {spelling}")
        elif spelling == "*":
            if before != ("symbol", "."):
                names.add(spelling)
        elif spelling in ("=", "<>", "<", "<=", ">", ">=", "/"):
            names.add(spelling)
    return names


def rule_ids(root):
    """Every rule ID a rule card anchors."""
    anchors = set()
    for path in (root / RULES).rglob("*.md"):
        text = path.read_text(encoding="utf-8")
        anchors.update(re.findall(r'<a id="([a-z]+-[0-9]+)"></a>', text))
    return {anchor.upper() for anchor in anchors}


def well_typed(kind, value):
    """Whether a YAML value is a value of a column type, or missing."""
    if value is None:
        return True
    if kind == "int":
        return type(value) is int and value in INT_RANGE
    if kind == "float":
        return type(value) is float and math.isfinite(value)
    if type(value) is not str:
        return False
    if kind == "str":
        return True
    pattern, parse = {
        "date": (DATE, dt.date.fromisoformat),
        "datetime": (DATETIME, dt.datetime.fromisoformat),
    }[kind]
    if not pattern.fullmatch(value):
        return False
    try:
        parse(value)
    except ValueError:
        return False
    return True


def table_errors(case, label):
    """Errors in a case's columns and its rows or groups."""
    errors = []
    columns = case.get("columns")
    if not isinstance(columns, dict):
        return [f"{label}: columns must map each column to its type"]
    for name, kind in columns.items():
        if not (isinstance(name, str) and COLUMN.fullmatch(name)):
            errors.append(f"{label}: column {name!r} is not an identifier")
        elif name in HARNESS_NAMES:
            errors.append(f"{label}: column {name} is a name the check reserves")
        if kind not in TYPES:
            errors.append(f"{label}: column {name!r} must have one of {TYPES}")
    if errors:
        return errors
    aggregate = case["context"] == "aggregate"
    key, other = ("groups", "rows") if aggregate else ("rows", "groups")
    if other in case:
        errors.append(
            f"{label}: a case read in {case['context']} gives {key}, not {other}"
        )
    entries = case.get(key)
    if not isinstance(entries, list) or not entries:
        return errors + [f"{label}: {key} must be a non-empty list"]
    records = []
    for index, entry in enumerate(entries, 1):
        if aggregate and not isinstance(entry, list):
            errors.append(f"{label}: group {index} must be a list of records")
        else:
            records.extend(entry if aggregate else [entry])
    for record in records:
        if not isinstance(record, dict) or set(record) != set(columns):
            errors.append(f"{label}: each record names exactly {sorted(columns)}")
            continue
        for name, value in record.items():
            if not well_typed(columns[name], value):
                errors.append(f"{label}: {name} = {value!r} is not a {columns[name]}")
            elif value == "":
                # An empty CSV field is missing (CSV-14), so no input holds
                # empty text.
                errors.append(f"{label}: {name} holds empty text; write null")
    grouping = case.get("group_by")
    if errors or grouping is None:
        return errors
    if not aggregate:
        return [f"{label}: only an aggregate case gives group_by"]
    if (
        not isinstance(grouping, list)
        or not grouping
        or len(set(grouping)) != len(grouping)
        or not set(grouping) <= set(columns)
    ):
        return [f"{label}: group_by must list distinct columns of the case"]
    # A grouping column has the one value its group's records share, so a
    # group that names one holds at least one record (FORMULA-22).
    for index, group in enumerate(entries, 1):
        if len({tuple(record[name] for name in grouping) for record in group}) != 1:
            errors.append(
                f"{label}: the records of group {index} must share one value of "
                "each group_by column"
            )
    return errors


def field_errors(case, label):
    """Errors in the reducer fields an aggregate case gives: order terms over
    the case's columns as R.<column>, a text separator, a Boolean distinct
    and a numeric quantile, which may lie outside 0 through 1 for a case that
    shows its refusal (VALUE-22, FORMULA-18)."""
    given = [key for key in REDUCER_FIELDS if key in case]
    if not given:
        return []
    if case["context"] != "aggregate":
        return [f"{label}: only an aggregate case gives {given}"]
    errors = []
    terms = case.get("order_by", [])
    if not isinstance(terms, list):
        errors.append(f"{label}: order_by must be a list of order terms")
        terms = []
    names = {f"{RELATION}.{name}" for name in case["columns"]}
    for term in terms:
        variable = term.get("variable") if isinstance(term, dict) else term
        if isinstance(term, dict) and (
            not set(term) <= ORDER_TERM_KEYS
            or term.get("direction", "asc") not in ("asc", "desc")
            or term.get("nulls", "first") not in ("first", "last")
        ):
            errors.append(f"{label}: order term {term!r} is not an order term")
        if variable not in names:
            errors.append(f"{label}: order term {term!r} names no column as R.<column>")
    if "separator" in case and not isinstance(case["separator"], str):
        errors.append(f"{label}: separator must be text")
    if "distinct" in case and not isinstance(case["distinct"], bool):
        errors.append(f"{label}: distinct must be true or false")
    quantile = case.get("quantile", 0.5)
    if type(quantile) not in (int, float) or not math.isfinite(quantile):
        errors.append(f"{label}: quantile must be a number")
    return errors


def case_errors(case, label, conditions, rules):
    """Errors in one case's keys, table and expectation."""
    errors = []
    unknown = sorted(set(case) - CASE_KEYS)
    if unknown:
        errors.append(f"{label}: unknown keys {unknown}")
    context = case.get("context")
    if context not in CONTEXTS:
        return errors + [f"{label}: context must be one of {CONTEXTS}"]
    if not isinstance(case.get("text"), str) or not case["text"]:
        errors.append(f"{label}: text must be a non-empty string")
    table = table_errors(case, label)
    if table:
        return errors + table
    fields = field_errors(case, label)
    if fields:
        return errors + fields
    kind = case.get("type")
    if "values" in case:
        if any(key in case for key in ISSUE_KEYS):
            errors.append(f"{label}: a case gives values or an issue, not both")
        if context == "predicate":
            if "type" in case:
                errors.append(f"{label}: a predicate's values are truth values")
            kind = None
        elif kind not in TYPES:
            errors.append(f"{label}: type must be one of {TYPES}")
            return errors
        values = case["values"]
        count = len(case.get("groups" if context == "aggregate" else "rows") or [])
        if not isinstance(values, list) or len(values) != count:
            return errors + [f"{label}: values must give one value per entry"]
        for value in values:
            valid = value in TRUTH if kind is None else well_typed(kind, value)
            if not valid:
                errors.append(f"{label}: {value!r} is not a {kind or 'truth'} value")
        return errors
    if kind is not None and (context == "predicate" or kind not in TYPES):
        errors.append(f"{label}: type must be one of {TYPES}, outside a predicate")
    missing = [key for key in ISSUE_KEYS if key not in case]
    if missing:
        return errors + [f"{label}: give values, or the issue's {missing}"]
    phases = conditions.get(case["condition"])
    if phases is None:
        errors.append(f"{label}: {case['condition']!r} is not a registered condition")
    elif case["phase"] not in phases:
        errors.append(f"{label}: {case['condition']} is raised only in {phases}")
    if not (isinstance(case["rule"], str) and RULE_ID.fullmatch(case["rule"])):
        errors.append(f"{label}: rule must be a rule ID")
    elif case["rule"] not in rules:
        errors.append(f"{label}: no rule card anchors {case['rule']}")
    return errors


def coverage_errors(cases, functions, reducers, operators):
    """Coverage the document promises and does not give."""
    errors = []
    valued = [case for case in cases if "values" in case]
    covered = {name for case in valued for name in case["covers"]}
    for kind, names in (
        ("function", functions),
        ("reducer", reducers),
        ("operator or literal", operators),
    ):
        missing = [name for name in names if name not in covered]
        if missing:
            errors.append(f"{DOCUMENT}: no value case covers the {kind}s {missing}")
    refused = {
        name
        for case in cases
        if case.get("condition") == "prohibited_function" and "arity" in case["covers"]
        for name in case["covers"]
    }
    missing = [name for name in functions if name not in refused]
    if missing:
        errors.append(
            f"{DOCUMENT}: no case refuses another argument count of {missing}"
        )
    for operator in LOGIC:
        truths = {
            value
            for case in valued
            if case["context"] == "predicate" and operator in case["covers"]
            for value in case["values"]
        }
        if set(TRUTH) - truths:
            errors.append(
                f"{DOCUMENT}: the predicate cases of {operator} give no "
                f"{sorted(set(TRUTH) - truths)}"
            )
    shown = {
        (case["context"], case.get("condition"), case.get("rule")) for case in cases
    }
    for refusal in REFUSALS:
        if refusal not in shown:
            errors.append(
                f"{DOCUMENT}: no {refusal[0]} case is refused with "
                f"{refusal[1]} under {refusal[2]}"
            )
    return errors


def check(document, root=ROOT):
    """Errors in the document's structure and coverage."""
    if not isinstance(document, dict) or set(document) != set(DOCUMENT_KEYS):
        return [f"{DOCUMENT}: the document's keys must be {list(DOCUMENT_KEYS)}"]
    grammar = yaml12.read_yaml(str(root / GRAMMAR))
    errors = []
    expected = {
        "schema_version": "1.0",
        "contract": "expressions",
        "grammar": {
            "contract": grammar["contract"],
            "contract_version": grammar["contract_version"],
        },
    }
    for key, value in expected.items():
        if document[key] != value:
            errors.append(f"{DOCUMENT}: {key} must be {value!r}")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", str(document["contract_version"])):
        errors.append(f"{DOCUMENT}: contract_version must be MAJOR.MINOR.PATCH")
    functions, reducers, operators = grammar_names(grammar)
    vocabulary = set(functions) | set(reducers) | set(operators) | TOPICS
    registry = yaml12.read_yaml(str(root / CONDITIONS))["conditions"]
    conditions = {name: entry["phases"] for name, entry in registry.items()}
    rules = rule_ids(root)
    cases = document["cases"]
    if not isinstance(cases, list) or not cases:
        return errors + [f"{DOCUMENT}: cases must be a non-empty list"]
    seen = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict) or not isinstance(case.get("id"), str):
            errors.append(f"{DOCUMENT}: cases[{index}] needs an id")
            continue
        label = f"{DOCUMENT}: {case['id']}"
        if not CASE_ID.fullmatch(case["id"]) or case["id"] in seen:
            errors.append(f"{label}: an id is unique lowercase words joined by -")
        seen.add(case["id"])
        covers = case.get("covers")
        if not isinstance(covers, list) or not covers:
            errors.append(f"{label}: covers must be a non-empty list")
        elif set(covers) - vocabulary:
            errors.append(f"{label}: unknown covers {sorted(set(covers) - vocabulary)}")
        elif isinstance(case.get("text"), str):
            # The coverage below counts these names, so each must be in the text.
            unwritten = sorted(set(covers) - TOPICS - written(case["text"]))
            if unwritten:
                errors.append(
                    f"{label}: covers {unwritten}, which its text does not write"
                )
        errors.extend(case_errors(case, label, conditions, rules))
    if errors:
        return errors
    read = {case["context"] for case in cases}
    errors.extend(
        f"{DOCUMENT}: no case is read in {context}"
        for context in CONTEXTS
        if context not in read
    )
    return errors + coverage_errors(cases, functions, reducers, operators)


def cell(kind, value):
    """A value as the case's CSV input holds it (CSV-4, VALUE-6, VALUE-15)."""
    if value is None:
        return ""
    if kind == "float":
        return repr(value)
    return str(value)


def aggregate(case):
    """A case's aggregate: its text alone, or, with the reducer fields it
    gives, the long form that holds them (SCHEMA-15)."""
    fields = {key: case[key] for key in REDUCER_FIELDS if key in case}
    return {"expr": case["text"], **fields} if fields else case["text"]


def write_csv(path, header, rows):
    with path.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(header)
        writer.writerows(rows)


def stage(case, directory):
    """Write the specification that runs a case, and its input, to `directory`.

    A predicate is a flag's condition, whose three values name the truth
    values (EXPR-8); derivation text is a column's derivation; aggregate text
    reduces the records of R that match each group's key (LOOKUP-18), or,
    for a case that names grouping columns, the records of each group a row
    template groups R by (FORMULA-22). Each column a predicate or derivation
    names is a column bound to its input.
    """
    columns = case["columns"]
    result = case.get("type", "float")
    if case.get("group_by"):
        return grouped_stage(case, directory)
    if case["context"] == "aggregate":
        groups = case["groups"]
        write_csv(
            directory / "groups.csv",
            ["GROUPID"],
            [[g] for g in range(1, len(groups) + 1)],
        )
        write_csv(
            directory / "records.csv",
            ["GROUPID", *columns],
            [
                [index, *(cell(kind, record[name]) for name, kind in columns.items())]
                for index, group in enumerate(groups, 1)
                for record in group
            ],
        )
        key = "GROUPID"
        inputs = {
            "GROUPS": {"path": "groups.csv", "types": {key: "int"}},
            RELATION: {
                "path": "records.csv",
                "types": {key: "int", **columns},
                "key": [key],
            },
        }
        base = "GROUPS"
        derived = [
            {
                "name": "RESULT",
                "type": result,
                "derivation": {"aggregate": aggregate(case)},
            }
        ]
    else:
        write_csv(
            directory / "rows.csv",
            ["ROWID", *columns],
            [
                [index, *(cell(kind, row[name]) for name, kind in columns.items())]
                for index, row in enumerate(case["rows"], 1)
            ],
        )
        key = "ROWID"
        inputs = {"SRC": {"path": "rows.csv", "types": {key: "int", **columns}}}
        base = "SRC"
        if case["context"] == "predicate":
            flag = dict(
                zip(("true_value", "false_value", "missing"), TRUTH, strict=True)
            )
            derivation = {"flag": {"condition": case["text"], **flag}}
            result = "str"
        else:
            derivation = case["text"]
        derived = [
            *(
                {"name": name, "type": kind, "derivation": f"SRC.{name}"}
                for name, kind in columns.items()
            ),
            {"name": "RESULT", "type": result, "derivation": derivation},
        ]
    specification = {
        "yamaa": "1.0",
        "type": "spec",
        "domain": "CONFORMANCE",
        "keys": [key],
        "input": inputs,
        "base": base,
        "columns": [
            {"name": key, "type": "int", "derivation": f"{base}.{key}"},
            *derived,
        ],
        "output": {"path": "unwritten.csv", "columns": [key, "RESULT"]},
    }
    path = directory / "spec.yaml"
    path.write_text(json.dumps(specification) + "\n", encoding="utf-8")
    return path


def grouped_stage(case, directory):
    """Write an aggregate case that names grouping columns as a row template
    that groups R by each group and those columns."""
    columns = case["columns"]
    write_csv(
        directory / "records.csv",
        ["GROUPID", *columns],
        [
            [index, *(cell(kind, record[name]) for name, kind in columns.items())]
            for index, group in enumerate(case["groups"], 1)
            for record in group
        ],
    )
    group_by = [f"{RELATION}.{name}" for name in ("GROUPID", *case["group_by"])]
    specification = {
        "yamaa": "1.0",
        "type": "spec",
        "domain": "CONFORMANCE",
        "keys": ["GROUPID"],
        "input": {
            RELATION: {"path": "records.csv", "types": {"GROUPID": "int", **columns}}
        },
        "columns": [
            {"name": "GROUPID", "type": "int", "derivation": f"{RELATION}.GROUPID"},
            {"name": "RESULT", "type": case.get("type", "float")},
        ],
        "rows": [
            {
                "id": "groups",
                "dataset": RELATION,
                "group_by": group_by,
                "derivations": {"RESULT": {"aggregate": aggregate(case)}},
            }
        ],
        "output": {"path": "unwritten.csv", "columns": ["GROUPID", "RESULT"]},
    }
    path = directory / "spec.yaml"
    path.write_text(json.dumps(specification) + "\n", encoding="utf-8")
    return path


def same(kind, expected, actual):
    """Whether a value read from the engine is the expected value exactly."""
    if expected is None or actual is None:
        return expected is None and actual is None
    if kind == "float":
        return type(actual) is float and struct.pack(">d", actual) == struct.pack(
            ">d", expected
        )
    if kind == "int":
        return type(actual) is int and actual == expected
    if kind == "date":
        return type(actual) is dt.date and actual == dt.date.fromisoformat(expected)
    if kind == "datetime":
        return type(actual) is dt.datetime and actual == dt.datetime.fromisoformat(
            expected
        )
    return actual == expected


def judge(case, status, output, issues):
    """The ways a run's status, output rows and issues differ from the case."""
    reported = [{key: issue[key] for key in ISSUE_KEYS} for issue in issues]
    if "values" not in case:
        expected = {key: case[key] for key in ISSUE_KEYS}
        if status == "failed" and reported == [expected]:
            return []
        return [f"expected the one issue {expected}, got {status} with {reported}"]
    if status != "succeeded" or reported:
        return [f"expected values, got {status} with {reported}"]
    key = "GROUPID" if case["context"] == "aggregate" else "ROWID"
    observed = {row[key]: row["RESULT"] for row in output}
    expected = dict(enumerate(case["values"], 1))
    if sorted(observed) != sorted(expected):
        return [f"expected entries {sorted(expected)}, got {sorted(observed)}"]
    kind = case.get("type", "str")
    return [
        f"entry {index}: expected {value!r}, got {observed[index]!r}"
        for index, value in expected.items()
        if not same(kind, value, observed[index])
    ]


def replay(document):
    """Run every case through the installed engine; the cases it does not reproduce."""
    try:
        import yamaa
    except ImportError as error:
        return [f"{DOCUMENT}: the engine is not installed ({error}); run make setup"]
    errors = []
    with tempfile.TemporaryDirectory(prefix="yamaa-expressions-") as scratch:
        for index, case in enumerate(document["cases"]):
            directory = Path(scratch) / f"case-{index:04d}"
            directory.mkdir()
            run = yamaa.spec(stage(case, directory)).run()
            output = run.output.to_pylist() if run.output is not None else []
            for problem in judge(case, run.status, output, run.issues.to_pylist()):
                errors.append(f"{DOCUMENT}: {case['id']}: {case['text']}: {problem}")
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    root = parser.parse_args(argv).root
    document, errors = load(root)
    if not errors:
        errors = check(document, root) or replay(document)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        sys.exit(1)
    print(
        f"PASS: {DOCUMENT}: the engine reproduces all "
        f"{len(document['cases'])} expression cases"
    )


if __name__ == "__main__":
    main()
