"""The CDISC vocabulary data file against the bundled Define-XML enumerations
and the schema types that spell its vocabularies."""

import shutil
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation import check_cdisc_vocabulary as OWNER
from tools.validation._repository_yaml import UniqueKeyLoader


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
        path = self.root / OWNER.VOCABULARY
        return yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)

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

    def test_every_vocabulary_names_its_schema_type_and_only_published_lists_an_enumeration(
        self,
    ):
        vocabularies = self.vocabulary()["vocabularies"]
        self.assertEqual(
            {name: entry.get("enumeration") for name, entry in vocabularies.items()},
            {
                "dataset_class": "ItemGroupClass",
                "dataset_subclass": "ItemGroupSubClass",
                "standard_name": "StandardName",
                "origin_type": "OriginType",
                "origin_source": "OriginSource",
                "role": None,
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
