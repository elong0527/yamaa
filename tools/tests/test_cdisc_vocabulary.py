"""The CDISC vocabulary data file against the bundled Define-XML enumerations
and the schema types and fields that spell its vocabularies."""

import copy
import shutil
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.tests import support
from tools.validation import check_cdisc_vocabulary as OWNER


class CdiscVocabularyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        for directory in OWNER.SCHEMA_SETS:
            (self.root / directory).mkdir(parents=True, exist_ok=True)
            for path in sorted((ROOT / directory).glob("schema*.yaml")):
                shutil.copy(path, self.root / directory / path.name)
        for relative in (OWNER.VOCABULARY, OWNER.ENUMERATIONS):
            (self.root / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / relative, self.root / relative)

    def tearDown(self):
        self.temporary.cleanup()

    def vocabulary(self):
        return support.load_yaml(self.root / OWNER.VOCABULARY)

    def write(self, document):
        (self.root / OWNER.VOCABULARY).write_text(
            yaml.safe_dump(document, sort_keys=False), encoding="utf-8"
        )

    def replace(self, relative, old, new):
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new, 1), encoding="utf-8")

    def test_the_repository_file_matches_its_enumerations_and_schemas(self):
        self.assertEqual(OWNER.check(ROOT), [])
        self.assertEqual(OWNER.check(self.root), [])

    def test_every_vocabulary_names_its_spelling_and_only_published_lists_an_enumeration(
        self,
    ):
        vocabularies = self.vocabulary()["vocabularies"]
        self.assertEqual(
            {
                name: (
                    entry.get("type") or entry.get("field"),
                    entry.get("enumeration"),
                    entry.get("schemas"),
                )
                for name, entry in vocabularies.items()
            },
            {
                "model": ("root_class.model", None, ["yaml/v1"]),
                "dataset_class": ("dataset_class_name", "ItemGroupClass", None),
                "dataset_subclass": (
                    "dataset_subclass_name",
                    "ItemGroupSubClass",
                    None,
                ),
                "standard_name": ("standard_name", "StandardName", None),
                # The 0.5 schema lists Draft and Final alone (P-STD-1).
                "standard_status": (
                    "implementation_guide_class.status",
                    "StandardStatus",
                    ["yaml/v1"],
                ),
                "publishing_set": (
                    "terminology_standard_class.publishing_set",
                    "StandardPublishingSet",
                    None,
                ),
                "origin_type": ("origin_type", "OriginType", None),
                "origin_source": ("origin_source", "OriginSource", None),
                "role": ("column_role", None, None),
                "core_designation": ("core_designation", None, None),
                "submission_data_type": ("define_data_type", None, None),
                "comparator": ("where_comparison_class.comparator", None, ["yaml/v1"]),
            },
        )
        self.assertEqual(
            OWNER.values(vocabularies["role"]),
            [
                "Identifier",
                "Topic",
                "Timing",
                "Grouping Qualifier",
                "Result Qualifier",
                "Synonym Qualifier",
                "Record Qualifier",
                "Variable Qualifier",
                "Rule",
            ],
        )

    def test_a_value_the_enumeration_lacks_or_adds_is_reported(self):
        document = self.vocabulary()
        classes = document["vocabularies"]["dataset_class"]["values"]
        classes.append({"value": "CUSTOM CLASS", "since": "1.0"})
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertTrue(
            any(
                "dataset_class: values differ from ItemGroupClass" in e for e in errors
            ),
            errors,
        )
        # Both schema sets still spell the published list.
        self.assertEqual(
            sum("dataset_class_name lists" in error for error in errors), 2, errors
        )

    def test_a_schema_type_that_drifts_from_the_file_is_reported_per_schema_set(self):
        self.replace(
            Path("yaml/v1/schema_metadata.yaml"),
            "Variable Qualifier, Rule]",
            "Variable Qualifier]",
        )
        errors = OWNER.check(self.root)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("yaml/v1/schema_metadata.yaml: column_role lists", errors[0])

    def test_a_vocabulary_whose_type_no_schema_set_declares_is_reported(self):
        document = self.vocabulary()
        document["vocabularies"]["role"]["type"] = "submission_role"
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertEqual(
            [error for error in errors if "has no type submission_role" in error],
            [
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.role.type: "
                f"{directory} has no type submission_role"
                for directory in OWNER.SCHEMA_SETS
            ],
        )

    def test_a_registry_type_lists_no_values(self):
        document = self.vocabulary()
        document["vocabularies"]["core_designation"]["type"] = "column_verification"
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertEqual(len(errors), 2, errors)
        self.assertTrue(
            errors[0].startswith(
                "ERROR: yaml/schema_verification.yaml: column_verification lists [], "
            ),
            errors,
        )

    def test_each_vocabulary_is_compared_with_each_schema_set_and_enumeration_it_names(
        self,
    ):
        metadata = ["yaml/schema_metadata.yaml", "yaml/v1/schema_metadata.yaml"]
        terminology = ["yaml/schema_project.yaml", "yaml/v1/schema_terminology.yaml"]
        # The modules that spell each vocabulary, in schema-set order.
        spelled = {
            "model": ["yaml/v1/schema_spec.yaml"],
            "dataset_class": metadata,
            "dataset_subclass": metadata,
            "standard_name": terminology,
            "standard_status": ["yaml/v1/schema_project.yaml"],
            "publishing_set": terminology,
            "origin_type": metadata,
            "origin_source": metadata,
            "role": metadata,
            "core_designation": metadata,
            "submission_data_type": metadata,
            "comparator": ["yaml/v1/schema_metadata.yaml"],
        }
        pristine = self.vocabulary()
        self.assertEqual(list(spelled), list(pristine["vocabularies"]))
        for name, modules in spelled.items():
            with self.subTest(name):
                document = copy.deepcopy(pristine)
                vocabulary = document["vocabularies"][name]
                vocabulary["values"].pop()
                self.write(document)
                expected = []
                if "enumeration" in vocabulary:
                    expected.append(
                        f"ERROR: {OWNER.VOCABULARY}.vocabularies.{name}: values differ "
                        f"from {vocabulary['enumeration']} in {OWNER.ENUMERATIONS}"
                    )
                spelling = vocabulary.get("type") or vocabulary["field"]
                expected += [f"ERROR: {module}: {spelling} lists" for module in modules]
                errors = OWNER.check(self.root)
                self.assertEqual(len(errors), len(expected), errors)
                for error, start in zip(errors, expected):
                    self.assertTrue(error.startswith(start), (error, start))

    def test_a_schema_field_that_drifts_from_the_file_is_reported_per_schema_set(self):
        self.replace(
            Path("yaml/v1/schema_project.yaml"),
            "values: [Draft, Final, Provisional]}",
            "values: [Draft, Final]}",
        )
        self.replace(Path("yaml/schema_project.yaml"), "    - DEFINE-XML\n", "")
        self.assertEqual(
            OWNER.check(self.root),
            [
                "ERROR: yaml/schema_project.yaml: "
                "terminology_standard_class.publishing_set lists "
                "['ADaM', 'CDASH', 'SDTM', 'SEND'], but "
                "yaml/vocabulary/cdisc.yaml.vocabularies.publishing_set lists "
                "['ADaM', 'CDASH', 'DEFINE-XML', 'SDTM', 'SEND']",
                "ERROR: yaml/v1/schema_project.yaml: "
                "implementation_guide_class.status lists ['Draft', 'Final'], but "
                "yaml/vocabulary/cdisc.yaml.vocabularies.standard_status lists "
                "['Draft', 'Final', 'Provisional']",
            ],
        )

    def test_schemas_limits_a_vocabulary_to_the_schema_sets_that_spell_it_so(self):
        document = self.vocabulary()
        for name in ("model", "standard_status", "comparator"):
            del document["vocabularies"][name]["schemas"]
        self.write(document)
        # The 0.5 schema has no model or comparator, and no Provisional.
        self.assertEqual(
            OWNER.check(self.root),
            [
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.model.field: "
                "yaml has no field root_class.model",
                "ERROR: yaml/schema_project.yaml: implementation_guide_class.status "
                "lists ['Draft', 'Final'], but "
                "yaml/vocabulary/cdisc.yaml.vocabularies.standard_status lists "
                "['Draft', 'Final', 'Provisional']",
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.comparator.field: "
                "yaml has no field where_comparison_class.comparator",
            ],
        )

    def test_a_vocabulary_whose_field_no_schema_set_declares_is_reported(self):
        document = self.vocabulary()
        vocabularies = document["vocabularies"]
        vocabularies["publishing_set"]["field"] = "terminology_standard_class.set"
        vocabularies["standard_status"]["field"] = "guide_class.status"
        self.write(document)
        label = "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies"
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{label}.publishing_set.field: "
                "yaml has no field terminology_standard_class.set",
                f"{label}.standard_status.field: "
                "yaml/v1 has no field guide_class.status",
                f"{label}.publishing_set.field: "
                "yaml/v1 has no field terminology_standard_class.set",
            ],
        )

    def test_a_vocabulary_names_one_spelling_and_distinct_known_schema_sets(self):
        document = self.vocabulary()
        vocabularies = document["vocabularies"]
        del vocabularies["model"]["field"]
        vocabularies["standard_status"]["schemas"] = ["yaml/v2"]
        vocabularies["publishing_set"]["schemas"] = []
        vocabularies["role"]["field"] = "submission_role_fields.role"
        vocabularies["core_designation"]["schemas"] = ["yaml/v1", "yaml/v1"]
        vocabularies["comparator"]["field"] = "comparator"
        self.write(document)
        label = "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies"
        sets = "expected distinct schema sets among ['yaml', 'yaml/v1']"
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{label}.model: expected exactly one of type and field",
                f"{label}.standard_status.schemas: {sets}",
                f"{label}.publishing_set.schemas: {sets}",
                f"{label}.role: expected exactly one of type and field",
                f"{label}.core_designation.schemas: {sets}",
                f"{label}.comparator.field: expected a class field as class.field",
            ],
        )

    def test_the_source_names_the_bundled_release_and_terminology_package(self):
        document = self.vocabulary()
        document["source"]["terminology"] = "2025-09-26"
        self.write(document)
        self.assertEqual(
            OWNER.check(self.root),
            [
                "ERROR: yaml/vocabulary/cdisc.yaml.source.terminology: "
                "'2025-09-26', but "
                "rust/tests/schemas/define/2.1/define-enumerations.xsd is "
                "'2026-03-27'"
            ],
        )

    def test_an_unknown_enumeration_is_reported(self):
        document = self.vocabulary()
        document["vocabularies"]["origin_source"]["enumeration"] = "OriginParty"
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertTrue(
            any("has no simple type OriginParty" in error for error in errors), errors
        )

    def test_values_are_unique_and_admitted_no_later_than_the_file_version(self):
        document = self.vocabulary()
        roles = document["vocabularies"]["role"]["values"]
        roles.append({"value": "Topic", "since": "1.0"})
        roles.append({"value": "Analysis", "since": "1.1"})
        roles.append({"value": "Grouping", "since": 1.0})
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertEqual(
            errors,
            [
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.role.values[9].value: "
                "'Topic' is listed twice",
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.role.values[10].since: "
                "1.1 is newer than the file's version 1.0",
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.role.values[11].since: "
                "expected a quoted 1.N version",
            ],
        )

    def test_the_file_version_is_the_1_0_schema_version(self):
        document = self.vocabulary()
        document["version"] = "1.1"
        self.write(document)
        errors = OWNER.check(self.root)
        self.assertTrue(errors)
        self.assertTrue(
            all(
                error.startswith("ERROR: yaml/v1/schema_")
                and error.endswith("but yaml/vocabulary/cdisc.yaml is '1.1'")
                for error in errors
            ),
            errors,
        )

    def test_unknown_fields_and_an_unreadable_file_are_reported(self):
        document = self.vocabulary()
        document["vocabularies"]["role"]["published_in"] = "SDTM v2.0"
        document["notes"] = "free text"
        self.write(document)
        self.assertEqual(
            OWNER.check(self.root),
            [
                "ERROR: yaml/vocabulary/cdisc.yaml.notes: unknown field",
                "ERROR: yaml/vocabulary/cdisc.yaml.vocabularies.role.published_in: "
                "unknown field",
            ],
        )
        (self.root / OWNER.VOCABULARY).write_text("a: 1\na: 2\n", encoding="utf-8")
        errors = OWNER.check(self.root)
        self.assertEqual(len(errors), 1)
        self.assertTrue(errors[0].startswith("ERROR: yaml/vocabulary/cdisc.yaml: "))


if __name__ == "__main__":
    unittest.main()
