"""The interface stability check's file-type schemas and grammars (#3030
P-STAB-1, manifest 1): one test per kind of break against a release tag in a
temporary repository, then additions, new and missing manifests, and this
repository compared with itself. Nothing here reads the network or this
repository's tags."""

import contextlib
import copy
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.packaging import generate_json_schema, release_identity
from tools.paths import ROOT
from tools.tests import support
from tools.tests.support import git, write
from tools.validation import check_interface_stability as STABILITY
from tools.validation import interface_schemas as SCHEMAS
from tools.validation.interface_manifests import ManifestError, Tree

TAG = "yamaa-v1.0.0"
SCHEMA = f"{SCHEMAS.JSON_SCHEMAS}/thing.schema.json"
IDENTIFIER = {"$ref": "#/$defs/identifier"}
DOCUMENT = {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "title": "yamaa 1.0 thing file",
    "description": "Generated.",
    "$ref": "#/$defs/thing_class",
    "$defs": {
        "identifier": {
            "type": "string",
            "pattern": "^[A-Za-z_][A-Za-z0-9_]*$",
            "description": "A name.",
        },
        "language_version": {"type": "string", "enum": ["1.0"]},
        # A registry: one keyword with a class payload, one with a value.
        "step": {
            "type": "object",
            "properties": {
                "copy": {
                    "type": "object",
                    "properties": {
                        "source": {
                            **IDENTIFIER,
                            "x-yamaa-kind": "name",
                            "x-yamaa-type": "identifier",
                        }
                    },
                    "required": ["source"],
                    "additionalProperties": False,
                },
                "drop": IDENTIFIER,
            },
            "additionalProperties": False,
            "minProperties": 1,
            "maxProperties": 1,
        },
        "thing_class": {
            "type": "object",
            "properties": {
                "yamaa": {
                    "$ref": "#/$defs/language_version",
                    "x-yamaa-kind": "value",
                    "x-yamaa-type": "language_version",
                },
                "name": {
                    **IDENTIFIER,
                    "description": "Name of the thing.",
                    "x-yamaa-kind": "identifier",
                    "x-yamaa-type": "identifier",
                },
                "mode": {
                    "type": "string",
                    "enum": ["fast", "slow"],
                    "default": "fast",
                    "x-yamaa-kind": "value",
                    "x-yamaa-type": "str",
                },
                "strict": {
                    "type": "boolean",
                    "default": False,
                    "x-yamaa-kind": "value",
                    "x-yamaa-type": "bool",
                },
                "values": {
                    "type": "array",
                    "items": {"type": ["string", "integer"]},
                    "x-yamaa-kind": "value",
                    "x-yamaa-type": "list[literal_value]",
                },
                "labels": {
                    "type": "object",
                    "propertyNames": IDENTIFIER,
                    "additionalProperties": {"type": "string"},
                    "x-yamaa-kind": "value",
                    "x-yamaa-type": "dict[identifier, str]",
                },
                "steps": {
                    "type": "array",
                    "items": {"$ref": "#/$defs/step"},
                    "x-yamaa-kind": "structure",
                    "x-yamaa-type": "list[step]",
                },
            },
            "required": ["yamaa", "name"],
            "additionalProperties": False,
        },
    },
}
EXPRESSION = """\
contract: expression
start: expression
productions:
  - name: expression
    definition: |-
      sum ["LIKE" sum ["ESCAPE" string]]
  - name: sum
    definition: |-
      primary (("+" | "-") primary)*
  - name: primary
    definition: |-
      call | reduction | name | string
  - name: call
    definition: |-
      function "(" [expression ("," expression)*] ")"
  - name: reduction
    definition: |-
      reducer "(" expression ")" | "COUNT" "(" name "." "*" ")"
  - name: name
    definition: |-
      letter { letter }
  - name: letter
    definition: |-
      "A" ... "Z" | "a" ... "z"
  - name: string
    definition: |-
      "'" { non_quote | "''" } "'"
  - name: non_quote
    prose: true
    definition: |-
      any Unicode scalar value other than "'"
imports: {}
vocabulary:
  function:
    ABS: {min_arguments: 1, max_arguments: 1, result: numeric}
    ROUND: {min_arguments: 1, max_arguments: 2, result: numeric}
    COALESCE: {min_arguments: 2, max_arguments: 2, result: numeric}
  reducer:
    SUM: {argument: [expression]}
    COUNT: {argument: [expression, star]}
reserved: ["LIKE"]
"""
TEMPLATE = """\
contract: string-template
start: template
productions:
  - name: template
    definition: |-
      part*
  - name: part
    definition: |-
      text | "{" variable "}" | "{{" | "}}"
  - name: text
    prose: true
    definition: |-
      one or more Unicode scalar values other than "{" and "}"
imports:
  variable: schema
vocabulary: {}
"""


def save_json(root, path, document):
    write(root, path, json.dumps(document, indent=2) + "\n")


class Repository(support.Repository):
    """A git repository whose tag yamaa-v1.0.0 holds a JSON Schema, both
    grammars and an empty exception list; the working tree starts the same."""

    def __init__(self, root, document=DOCUMENT):
        super().__init__(root)
        save_json(root, SCHEMA, document)
        self.write(SCHEMAS.EXPRESSION, EXPRESSION)
        self.write(SCHEMAS.TEMPLATE, TEMPLATE)
        self.write(STABILITY.EXCEPTIONS, "exceptions: []\n")
        self.tag(TAG)

    def document(self):
        return json.loads((self.root / SCHEMA).read_text(encoding="utf-8"))

    def save(self, document):
        save_json(self.root, SCHEMA, document)

    def grammar(self, path):
        return yaml.safe_load((self.root / path).read_text(encoding="utf-8"))

    def save_grammar(self, path, document):
        write(self.root, path, yaml.safe_dump(document, sort_keys=False))

    def changes(self):
        """Each change against the tag as (element, what, breaking), by
        manifest."""
        changes = {}
        for change in SCHEMAS.compare(Tree(self.root, TAG), Tree(self.root)):
            changes.setdefault(change.manifest, []).append(
                (change.element, change.what, change.breaking)
            )
        return changes


class Temporary(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)

    def repository(self, document=DOCUMENT, *, name="repo"):
        """The repository, then its working tree's JSON Schema to edit and
        save: the document, its named types and the fields of thing_class."""
        repo = Repository(self.directory / name, document)
        document = repo.document()
        definitions = document["$defs"]
        return repo, document, definitions, definitions["thing_class"]["properties"]


class Inventories(Temporary):
    def test_a_schema_is_an_inventory_of_named_types_fields_and_keywords(self):
        save_json(self.directory, SCHEMA, DOCUMENT)
        inventory = SCHEMAS.schema_inventory(Tree(self.directory), SCHEMA)
        self.assertEqual(
            [
                (element, SCHEMAS._render(facts["type"]), facts["required"])
                for element, facts in inventory.items()
            ],
            [
                ("$ref", "thing_class", False),
                ("identifier", "string", False),
                ("language_version", "string", False),
                ("step", "object", False),
                ("step.copy", "object", False),
                ("step.copy.source", "identifier", True),
                ("step.drop", "identifier", False),
                ("thing_class", "object", False),
                ("thing_class.yamaa", "language_version", True),
                ("thing_class.name", "identifier", True),
                ("thing_class.mode", "string", False),
                ("thing_class.strict", "boolean", False),
                ("thing_class.values", "list[integer | string]", False),
                ("thing_class.labels", "dict[identifier, string]", False),
                ("thing_class.steps", "list[step]", False),
            ],
        )
        self.assertEqual(
            inventory["thing_class.mode"],
            {
                "parent": "thing_class",
                "type": frozenset(["string"]),
                "required": False,
                "kind": "value",
                "default": '"fast"',
                "enum": ['"fast"', '"slow"'],
                "extra": None,
                "pattern": None,
                **dict.fromkeys(SCHEMAS.LOWER_BOUNDS + SCHEMAS.UPPER_BOUNDS),
                "other": {},
            },
        )
        self.assertEqual(
            {
                key: inventory["step"][key]
                for key in ("extra", "minProperties", "maxProperties")
            },
            {"extra": "closed", "minProperties": 1, "maxProperties": 1},
        )
        self.assertEqual(SCHEMAS.schema_inventory(Tree(self.directory), "absent"), {})

    def test_a_grammar_is_an_inventory_of_its_vocabulary_terminals_and_productions(
        self,
    ):
        write(self.directory, SCHEMAS.EXPRESSION, EXPRESSION)
        write(self.directory, SCHEMAS.TEMPLATE, TEMPLATE)
        tree = Tree(self.directory)
        inventory = SCHEMAS.grammar_inventory(tree, SCHEMAS.EXPRESSION)
        self.assertEqual(
            {element: facts for element, facts in inventory.items() if facts},
            {
                "function ABS": (1, 1, "numeric"),
                "function ROUND": (1, 2, "numeric"),
                "function COALESCE": (2, 2, "numeric"),
                "reducer SUM": ("expression",),
                "reducer COUNT": ("expression", "star"),
            },
        )
        # A range's bounds and a prose definition's quotes are not terminals.
        self.assertEqual(
            [element for element, facts in inventory.items() if not facts],
            [
                'keyword "LIKE"',
                'keyword "ESCAPE"',
                'keyword "COUNT"',
                'operator "+"',
                'operator "-"',
                'operator "("',
                'operator ","',
                'operator ")"',
                'operator "."',
                'operator "*"',
                'operator "\'"',
                "operator \"''\"",
                "production expression",
                "production sum",
                "production primary",
                "production call",
                "production reduction",
                "production name",
                "production letter",
                "production string",
                "production non_quote",
                'reserved "LIKE"',
            ],
        )
        self.assertEqual(
            list(SCHEMAS.grammar_inventory(tree, SCHEMAS.TEMPLATE)),
            [
                'operator "{"',
                'operator "}"',
                'operator "{{"',
                'operator "}}"',
                "production template",
                "production part",
                "production text",
            ],
        )
        self.assertIsNone(SCHEMAS.grammar_inventory(tree, "absent.yaml"))


class Breaks(Temporary):
    """One test per kind of break, each with the additions beside it."""

    def test_a_removed_or_renamed_field_or_class_breaks(self):
        repo, document, definitions, fields = self.repository()
        del fields["values"]
        definitions["stage"] = definitions.pop("step")
        fields["steps"]["items"]["$ref"] = "#/$defs/stage"
        repo.save(document)
        # The renamed registry's keywords and fields are not listed again.
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    ("step", "removed", True),
                    ("thing_class.values", "removed", True),
                    (
                        "thing_class.steps",
                        "type changed from `list[step]` to `list[stage]`",
                        True,
                    ),
                    ("stage", "added", False),
                ]
            },
        )

    def test_a_narrowed_type_breaks(self):
        repo, document, definitions, fields = self.repository()
        fields["values"]["items"]["type"] = "string"
        fields["name"] = {"type": "string", "x-yamaa-kind": "identifier"}
        definitions["identifier"].update(minLength=1, pattern="^[A-Za-z]+$")
        fields["labels"]["additionalProperties"]["type"] = ["string", "null"]
        definitions["step"]["maxProperties"] = 2
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    ("identifier", "minLength changed from none to `1`", True),
                    (
                        "identifier",
                        "pattern changed from `^[A-Za-z_][A-Za-z0-9_]*$` to "
                        "`^[A-Za-z]+$`",
                        True,
                    ),
                    ("step", "maxProperties changed from `1` to `2`", False),
                    (
                        "thing_class.name",
                        "type changed from `identifier` to `string`",
                        True,
                    ),
                    (
                        "thing_class.values",
                        "type narrowed from `list[integer | string]` to `list[string]`",
                        True,
                    ),
                    (
                        "thing_class.labels",
                        "type widened from `dict[identifier, string]` to "
                        "`dict[identifier, null | string]`",
                        False,
                    ),
                ]
            },
        )

    def test_a_changed_field_kind_breaks(self):
        repo, document, _, fields = self.repository()
        fields["name"]["x-yamaa-kind"] = "name"
        # Neither prose nor the notation type is compared.
        fields["name"]["description"] = "Another description."
        fields["steps"]["x-yamaa-type"] = "list[step_entry]"
        document["description"] = "Generated differently."
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "thing_class.name",
                        "kind changed from `identifier` to `name`",
                        True,
                    )
                ]
            },
        )

    def test_a_new_required_field_breaks(self):
        repo, document, definitions, fields = self.repository()
        payload = definitions["step"]["properties"]["copy"]
        payload["properties"]["target"] = IDENTIFIER
        payload["required"].append("target")
        fields["owner"] = {**IDENTIFIER, "x-yamaa-kind": "identifier"}
        fields["note"] = {"type": "string", "x-yamaa-kind": "value"}
        definitions["thing_class"]["required"] = ["yamaa", "mode", "owner"]
        # A new class may require its fields: nothing wrote it before.
        definitions["extra_class"] = {
            "type": "object",
            "properties": {"id": IDENTIFIER},
            "required": ["id"],
            "additionalProperties": False,
        }
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    ("thing_class.name", "now optional", False),
                    ("thing_class.mode", "now required", True),
                    ("step.copy.target", "added, required", True),
                    ("thing_class.owner", "added, required", True),
                    ("thing_class.note", "added", False),
                    ("extra_class", "added", False),
                ]
            },
        )

    def test_a_changed_default_breaks(self):
        repo, document, _, fields = self.repository()
        fields["mode"]["default"] = "slow"
        del fields["strict"]["default"]
        fields["values"]["default"] = []
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "thing_class.mode",
                        'default changed from `"fast"` to `"slow"`',
                        True,
                    ),
                    (
                        "thing_class.strict",
                        "default changed from `false` to none",
                        True,
                    ),
                    ("thing_class.values", "default changed from none to `[]`", True),
                ]
            },
        )

    def test_a_removed_enumerated_value_breaks(self):
        repo, document, definitions, fields = self.repository()
        definitions["language_version"]["enum"] = ["1.1", "1.0"]
        fields["mode"]["enum"] = ["fast"]
        definitions["identifier"]["enum"] = ["a", "b"]
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "identifier",
                        'now restricted to enumerated values `"a"`, `"b"`',
                        True,
                    ),
                    ("language_version", 'enumerated value `"1.1"` added', False),
                    ("thing_class.mode", 'enumerated value `"slow"` removed', True),
                ]
            },
        )
        document = copy.deepcopy(DOCUMENT)
        del document["$defs"]["thing_class"]["properties"]["mode"]["enum"]
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "thing_class.mode",
                        "no longer restricted to enumerated values",
                        False,
                    )
                ]
            },
        )

    def test_a_tightened_extra_property_rule_breaks(self):
        # The baseline leaves the payload open: it admits properties it does
        # not list.
        document = copy.deepcopy(DOCUMENT)
        del document["$defs"]["step"]["properties"]["copy"]["additionalProperties"]
        repo, document, definitions, _ = self.repository(document)
        definitions["step"]["properties"]["copy"]["additionalProperties"] = False
        definitions["thing_class"]["additionalProperties"] = True
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    ("step.copy", "extra properties now refused", True),
                    ("thing_class", "extra properties now admitted", False),
                ]
            },
        )

    def test_root_schema_constraints_are_compared(self):
        repo, document, _, _ = self.repository()
        document["allOf"] = [False]
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {SCHEMA: [("$ref", "allOf changed from none to `[false]`", True)]},
        )

    def test_conditional_wrappers_keep_the_predicate_and_both_branches(self):
        # Seam: schema inventory comparisons against an authored release,
        # including a wrapper with no direct type or reference.
        document = copy.deepcopy(DOCUMENT)
        wrapper = {
            "if": {"required": ["parent"]},
            "then": {"$ref": "#/$defs/identifier"},
            "else": {"$ref": "#/$defs/thing_class"},
        }
        document["$defs"]["thing_file"] = wrapper
        document["$ref"] = "#/$defs/thing_file"
        repo, current, definitions, _ = self.repository(document)
        self.assertEqual(repo.changes(), {})
        for keyword, replacement, what in (
            (
                "if",
                {"required": ["clear"]},
                'if changed from `{"required": ["parent"]}` '
                'to `{"required": ["clear"]}`',
            ),
            (
                "then",
                {"anyOf": [{"$ref": "#/$defs/identifier"}]},
                'then changed from `{"$ref": "#/$defs/identifier"}` '
                'to `{"anyOf": [{"$ref": "#/$defs/identifier"}]}`',
            ),
            (
                "else",
                {"anyOf": [{"$ref": "#/$defs/thing_class"}]},
                'else changed from `{"$ref": "#/$defs/thing_class"}` '
                'to `{"anyOf": [{"$ref": "#/$defs/thing_class"}]}`',
            ),
        ):
            with self.subTest(keyword=keyword):
                definitions["thing_file"] = {**wrapper, keyword: replacement}
                repo.save(current)
                self.assertEqual(repo.changes(), {SCHEMA: [("thing_file", what, True)]})

    def test_schema_valued_extra_property_rules_are_compared(self):
        document = copy.deepcopy(DOCUMENT)
        document["$defs"]["thing_class"]["additionalProperties"] = {"type": "string"}
        repo, document, definitions, _ = self.repository(document)
        for rule, what, breaking in (
            (
                {"type": "integer"},
                'extra-property rule changed from `{"type": "string"}` '
                'to `{"type": "integer"}`',
                True,
            ),
            (False, "extra properties now refused", True),
            (True, "extra properties now admitted", False),
            ({}, "extra properties now admitted", False),
        ):
            with self.subTest(rule=rule):
                definitions["thing_class"]["additionalProperties"] = rule
                repo.save(document)
                self.assertEqual(
                    repo.changes(), {SCHEMA: [("thing_class", what, breaking)]}
                )

        definitions["thing_class"]["additionalProperties"] = {"type": "string"}
        definitions["step"]["additionalProperties"] = {"type": "string"}
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "step",
                        "extra-property rule changed from `closed` "
                        'to `{"type": "string"}`',
                        False,
                    )
                ]
            },
        )

    def test_unsupported_type_keyword_conjunctions_fail_closed(self):
        repo, document, _, fields = self.repository()
        for node, message in (
            ({**IDENTIFIER, "type": "integer"}, "type alongside \\$ref"),
            (
                {"anyOf": [{"type": "string"}], "type": "integer"},
                "type alongside anyOf",
            ),
            ({**IDENTIFIER, "items": {"type": "integer"}}, "items alongside \\$ref"),
        ):
            with self.subTest(node=node):
                fields["name"] = node
                repo.save(document)
                with self.assertRaisesRegex(ManifestError, message):
                    repo.changes()

    def test_extra_property_schema_prose_is_not_compared(self):
        for name, wrap in (
            ("direct", lambda value: value),
            ("items", lambda value: {"type": "array", "items": value}),
            ("anyOf", lambda value: {"anyOf": [value, {"type": "null"}]}),
            (
                "properties",
                lambda value: {"type": "object", "properties": {"description": value}},
            ),
        ):
            with self.subTest(schema=name):
                document = copy.deepcopy(DOCUMENT)
                leaf = {"type": "string"}
                rule = wrap(leaf)
                document["$defs"]["thing_class"]["additionalProperties"] = rule
                repo, current, definitions, _ = self.repository(document, name=name)
                leaf.update({keyword: "New prose" for keyword in SCHEMAS.PROSE})
                definitions["thing_class"]["additionalProperties"] = rule
                repo.save(current)
                self.assertEqual(repo.changes(), {})

        document = copy.deepcopy(DOCUMENT)
        document["$defs"]["thing_class"]["additionalProperties"] = True
        repo, current, definitions, _ = self.repository(document)
        definitions["thing_class"]["additionalProperties"] = {
            keyword: "Only prose" for keyword in SCHEMAS.PROSE
        }
        repo.save(current)
        self.assertEqual(repo.changes(), {})

    def test_extra_property_schema_data_and_property_names_are_compared(self):
        for index, rule in enumerate(
            (
                {"enum": [{"description": "before"}]},
                {"default": {"title": "before"}},
                {"type": "object", "properties": {"description": {"type": "string"}}},
            )
        ):
            with self.subTest(rule=rule):
                document = copy.deepcopy(DOCUMENT)
                document["$defs"]["thing_class"]["additionalProperties"] = rule
                repo, current, definitions, _ = self.repository(
                    document, name=f"data-{index}"
                )
                definitions["thing_class"]["additionalProperties"] = json.loads(
                    json.dumps(rule)
                    .replace("before", "after")
                    .replace('"description": {', '"title": {')
                )
                repo.save(current)
                self.assertTrue(repo.changes()[SCHEMA][0][2])

    def test_nested_prose_in_conservative_constraints_is_not_compared(self):
        document = copy.deepcopy(DOCUMENT)
        document["allOf"] = [{"type": "string"}]
        document["$defs"]["thing_class"]["propertyNames"] = {"pattern": "^x$"}
        repo, current, definitions, _ = self.repository(document)
        current["allOf"][0]["description"] = "Root documentation."
        definitions["thing_class"]["propertyNames"]["title"] = "A property name."
        repo.save(current)
        self.assertEqual(repo.changes(), {})

    def test_unsupported_object_constraints_fail_closed(self):
        repo, document, definitions, _ = self.repository()
        for constraints, message in (
            ({"required": ["strict"]}, "required alongside \\$ref"),
            (
                {"properties": {"name": {"type": "integer"}}},
                "properties alongside \\$ref",
            ),
        ):
            with self.subTest(constraints=constraints):
                repo.save({**document, **constraints})
                with self.assertRaisesRegex(ManifestError, message):
                    repo.changes()
        definitions["thing_class"]["required"].append("undeclared")
        repo.save(document)
        with self.assertRaisesRegex(
            ManifestError, "required names must have property schemas"
        ):
            repo.changes()

    def test_record_property_name_constraints_are_compared(self):
        repo, document, definitions, _ = self.repository()
        definitions["thing_class"]["propertyNames"] = {"pattern": "^x$"}
        repo.save(document)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMA: [
                    (
                        "thing_class",
                        'propertyNames changed from none to `{"pattern": "^x$"}`',
                        True,
                    )
                ]
            },
        )

    def test_a_removed_grammar_element_breaks(self):
        repo, *_ = self.repository()
        grammar = repo.grammar(SCHEMAS.EXPRESSION)
        del grammar["vocabulary"]["function"]["ABS"]
        del grammar["vocabulary"]["reducer"]["SUM"]
        productions = {item["name"]: item for item in grammar["productions"]}
        productions["expression"]["definition"] = 'sum ["LIKE" sum]'
        productions["sum"]["definition"] = 'primary ("+" primary)*'
        productions["primary"]["definition"] = "reduction | name | string"
        grammar["productions"].remove(productions["call"])
        repo.save_grammar(SCHEMAS.EXPRESSION, grammar)
        template = repo.grammar(SCHEMAS.TEMPLATE)
        template["productions"][1]["definition"] = 'text | "{" variable "}" | "}}"'
        repo.save_grammar(SCHEMAS.TEMPLATE, template)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMAS.EXPRESSION: [
                    ("function ABS", "removed", True),
                    ("reducer SUM", "removed", True),
                    ('keyword "ESCAPE"', "removed", True),
                    ('operator "-"', "removed", True),
                    ('operator ","', "removed", True),
                    ("production call", "removed", True),
                ],
                SCHEMAS.TEMPLATE: [('operator "{{"', "removed", True)],
            },
        )

    def test_a_narrowed_function_or_reducer_and_a_new_reserved_word_break(self):
        repo, *_ = self.repository()
        grammar = repo.grammar(SCHEMAS.EXPRESSION)
        functions = grammar["vocabulary"]["function"]
        functions["ABS"]["result"] = "boolean"
        functions["ROUND"]["max_arguments"] = 1
        functions["COALESCE"]["max_arguments"] = None
        reducers = grammar["vocabulary"]["reducer"]
        reducers["SUM"]["argument"].append("star")
        reducers["COUNT"]["argument"].remove("star")
        # A reserved word refuses a bare identifier: adding one breaks.
        grammar["reserved"] = ["ESCAPE"]
        repo.save_grammar(SCHEMAS.EXPRESSION, grammar)
        self.assertEqual(
            repo.changes(),
            {
                SCHEMAS.EXPRESSION: [
                    (
                        "function ABS",
                        "changed from `1 argument, numeric` to `1 argument, boolean`",
                        True,
                    ),
                    (
                        "function ROUND",
                        "changed from `1 to 2 arguments, numeric` to "
                        "`1 argument, numeric`",
                        True,
                    ),
                    (
                        "function COALESCE",
                        "changed from `2 arguments, numeric` to "
                        "`2 or more arguments, numeric`",
                        False,
                    ),
                    ("reducer SUM", "argument form `star` added", False),
                    ("reducer COUNT", "argument form `star` removed", True),
                    (
                        'reserved "LIKE"',
                        "removed: a bare identifier may take it",
                        False,
                    ),
                    (
                        'reserved "ESCAPE"',
                        "added: a bare identifier may no longer take it",
                        True,
                    ),
                ]
            },
        )


class Manifests(Temporary):
    def test_a_manifest_the_baseline_lacks_is_all_additions(self):
        root = support.init(self.directory / "repo")
        write(root, "README.md", "A release before the manifests.\n")
        git(root, "add", "-A")
        git(root, "commit", "-q", "-m", "release")
        git(root, "tag", "-a", TAG, "-m", TAG)
        save_json(root, SCHEMA, DOCUMENT)
        write(root, SCHEMAS.EXPRESSION, EXPRESSION)
        write(root, SCHEMAS.TEMPLATE, TEMPLATE)
        changes = SCHEMAS.compare(Tree(root, TAG), Tree(root))
        self.assertFalse([change for change in changes if change.breaking])
        self.assertEqual({change.what for change in changes}, {"added"})
        # A new JSON Schema lists its named types; their fields are not
        # listed again. A new grammar lists every element, its reserved
        # words included.
        self.assertEqual(
            [change.element for change in changes if change.manifest == SCHEMA],
            ["$ref", "identifier", "language_version", "step", "thing_class"],
        )
        for path in SCHEMAS.GRAMMARS:
            with self.subTest(path=path):
                self.assertEqual(
                    [change.element for change in changes if change.manifest == path],
                    list(SCHEMAS.grammar_inventory(Tree(root), path)),
                )

    def test_a_removed_file_type_breaks_and_a_missing_manifest_fails(self):
        other = f"{SCHEMAS.JSON_SCHEMAS}/other.schema.json"
        repo, *_ = self.repository()
        save_json(repo.root, other, DOCUMENT)
        git(repo.root, "add", "-A")
        git(repo.root, "commit", "-q", "-m", "another file type")
        git(repo.root, "tag", "-f", "-a", TAG, "-m", TAG)
        (repo.root / other).unlink()
        self.assertEqual(
            repo.changes(),
            {
                other: [
                    (element, "removed", True)
                    for element in (
                        *("$ref", "identifier", "language_version"),
                        *("step", "thing_class"),
                    )
                ]
            },
        )
        (repo.root / SCHEMA).unlink()
        with self.assertRaisesRegex(
            ManifestError, "yaml/json: no JSON Schema in the working tree"
        ):
            repo.changes()
        save_json(repo.root, SCHEMA, DOCUMENT)
        (repo.root / SCHEMAS.TEMPLATE).unlink()
        with self.assertRaisesRegex(
            ManifestError,
            "yaml/grammar/string-template.yaml: missing from the working tree",
        ):
            repo.changes()

    def test_a_malformed_manifest_fails(self):
        repo, document, _, fields = self.repository()
        for path, text, message in (
            (SCHEMA, "{", "Expecting property name"),
            (SCHEMA, "[]", "expected `\\$defs`"),
            (SCHEMA, '{"$defs": {}}', "\\$ref: expected a reference into \\$defs"),
            (SCHEMAS.EXPRESSION, "productions: {}\n", "expected productions"),
            (SCHEMAS.EXPRESSION, "a: 1\na: 2\n", "duplicate key"),
        ):
            with self.subTest(path=path, text=text):
                original = (repo.root / path).read_text(encoding="utf-8")
                write(repo.root, path, text)
                with self.assertRaisesRegex(ManifestError, message):
                    repo.changes()
                write(repo.root, path, original)
        # A type keyword the comparer does not read inside a type fails
        # rather than going unchecked.
        fields["values"]["items"]["minLength"] = 1
        repo.save(document)
        with self.assertRaisesRegex(
            ManifestError, "thing_class.values: cannot compare minLength in a type"
        ):
            repo.changes()


def run_check(root):
    """Run the check with this comparer alone, as the release at `root`."""
    output = io.StringIO()
    with (
        mock.patch.object(STABILITY, "COMPARERS", (SCHEMAS.__name__,)),
        contextlib.redirect_stdout(output),
    ):
        status = STABILITY.main(["--root", str(root)])
    return status, output.getvalue()


class Check(Temporary):
    def test_the_check_refuses_a_break_within_the_major_except_an_excepted_one(self):
        repo, document, _, fields = self.repository()
        del fields["values"]
        fields["mode"]["default"] = "slow"
        repo.save(document)
        write(repo.root, release_identity.DECLARATION, 'version = "1.0.1"\n')
        entry = {
            "manifest": SCHEMA,
            "element": "thing_class.mode",
            "rule": "CT-13",
            "issue": 1,
        }
        write(repo.root, STABILITY.EXCEPTIONS, yaml.safe_dump({"exceptions": [entry]}))
        status, output = run_check(repo.root)
        self.assertEqual(status, 1, output)
        self.assertEqual(
            output.splitlines(),
            [
                "Breaking changes against yamaa-v1.0.0:",
                f"{SCHEMA}: thing_class.values: removed",
                "Breaking changes against yamaa-v1.0.0 that "
                "tools/validation/interface_exceptions.yaml excepts:",
                f'{SCHEMA}: thing_class.mode: default changed from `"fast"` to '
                '`"slow"` (CT-13, #1)',
                "ERROR: 1 breaking change against yamaa-v1.0.0, the newest "
                "release of major 1. Only a new major release may break the "
                "frozen interface; a reviewed defect fix lists the element it "
                "changes in tools/validation/interface_exceptions.yaml",
            ],
        )

    def test_main_compared_with_itself_finds_nothing(self):
        self.assertIn(SCHEMAS.__name__, STABILITY.COMPARERS)
        tree = Tree(ROOT)
        self.assertEqual(SCHEMAS.compare(tree, tree), [])
        # The comparer reads every JSON Schema the generator writes, where it
        # writes them, and both grammars.
        self.assertEqual(
            SCHEMAS.schema_paths(tree),
            sorted(
                generate_json_schema.target(file_type).relative_to(ROOT).as_posix()
                for file_type in generate_json_schema.FILE_TYPES
            ),
        )
        for path in SCHEMAS.schema_paths(tree):
            self.assertIn("$ref", SCHEMAS.schema_inventory(tree, path))
        for path in SCHEMAS.GRAMMARS:
            self.assertTrue(SCHEMAS.grammar_inventory(tree, path))


if __name__ == "__main__":
    unittest.main()
