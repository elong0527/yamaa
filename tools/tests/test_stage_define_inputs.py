"""Exercise independent Define oracles after staging outside the installed package."""

import json
import tempfile
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError
from lxml import etree

from tools.paths import ROOT
from tools.staging import stage_define_inputs as stage


class DefineStage(unittest.TestCase):
    def test_staged_oracles_validate_goldens_and_reject_invalid_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "define-fixtures"
            stage.stage(destination)
            schemas = destination / "schemas"
            originals = ROOT / "rust/tests/schemas"
            for original in originals.rglob("*"):
                if original.is_file():
                    self.assertEqual(
                        (schemas / original.relative_to(originals)).read_bytes(),
                        original.read_bytes(),
                    )
            parser = etree.XMLParser(no_network=True, resolve_entities=False)
            legacy_schema = etree.XMLSchema(
                etree.parse(str(schemas / "define/2.0/define2-0-0.xsd"), parser)
            )
            # These independently authored documents are also the application
            # fixture reader's exact-byte expectations for the native writer.
            for name in (
                "define_2_0.xml",
                "define_2_0_historical.xml",
                "define_2_0_historical_no_artifact.xml",
                "define_2_0_historical_values.xml",
                "define_2_0_historical_shared_output.xml",
                "define_2_0_send.xml",
            ):
                legacy_schema.assertValid(
                    etree.parse(
                        str(ROOT / "rust/crates/yamaa-adapters/tests/fixtures" / name),
                        parser,
                    )
                )
            xml_schema = etree.XMLSchema(
                etree.parse(str(schemas / "define/2.1/define2-1-0.xsd"), parser)
            )
            # The non-standard dataset's document, authored the same way.
            xml_schema.assertValid(
                etree.parse(
                    str(
                        ROOT
                        / "rust/crates/yamaa-adapters/tests/fixtures"
                        / "define_2_1_non_standard.xml"
                    ),
                    parser,
                )
            )
            expected = destination / "sdtm-dm-metadata/expected"
            xml = etree.parse(str(expected / "define.xml"), parser)
            xml_schema.assertValid(xml)
            xml.getroot().set("FileType", "invalid")
            self.assertFalse(xml_schema.validate(xml))
            # A document validates against the schema of the Define-XML version
            # its `def` namespace names.
            define_schemas = {
                "http://www.cdisc.org/ns/def/v2.0": legacy_schema,
                "http://www.cdisc.org/ns/def/v2.1": xml_schema,
            }
            validator = Draft202012Validator(
                json.loads((schemas / "dataset.schema.json").read_bytes())
            )
            dataset = json.loads((expected / "dm.json").read_bytes())
            validator.validate(dataset)
            dataset["records"] = -1
            with self.assertRaises(ValidationError):
                validator.validate(dataset)
            # Every staged Define benchmark: positive goldens are schema valid,
            # and a negative case carries its expected first issue.
            benchmarks = destination / "benchmarks"
            manifest = json.loads((benchmarks / "manifest.json").read_bytes())
            cases = {case["name"]: case for case in manifest["cases"]}
            self.assertEqual(cases["sdtm-dm-xpt"]["expected"], ["define.xml", "dm.xpt"])
            self.assertEqual(
                cases["schema-transport-dataset-ndjson"]["expected"],
                ["define.xml", "dm.ndjson"],
            )
            self.assertEqual(cases["adam-adsl-xpt"]["standard"], "adam")
            self.assertEqual(cases["adam-adsl-xpt"]["project"], "project.yaml")
            self.assertEqual(
                cases["negative-transport-empty-string"]["published"], ["dm.parquet"]
            )
            for name, case in cases.items():
                if case["error"] is not None:
                    self.assertTrue(name.startswith("negative-"), name)
                    self.assertTrue(
                        {"phase", "condition", "spec_paths"} <= set(case["error"])
                    )
                    continue
                expected = benchmarks / name / "expected"
                document = etree.parse(str(expected / "define.xml"), parser)
                define_schemas[document.getroot().nsmap["def"]].assertValid(document)
                for document in expected.glob("*.json"):
                    validator.validate(json.loads(document.read_bytes()))
                # A newline-delimited golden is its first line with the row
                # lines appended as `rows` (JSON-4).
                for document in expected.glob("*.ndjson"):
                    first, *rows = document.read_text(encoding="utf-8").splitlines()
                    dataset = json.loads(first)
                    dataset["rows"] = [json.loads(row) for row in rows]
                    validator.validate(dataset)


if __name__ == "__main__":
    unittest.main()
