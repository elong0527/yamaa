"""Exercise independent Define oracles after staging outside the installed package."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, ValidationError
from lxml import etree

ROOT = Path(__file__).parents[2]
spec = importlib.util.spec_from_file_location(
    "stage_define_inputs", ROOT / "rust/tools/stage_define_inputs.py"
)
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


class DefineStage(unittest.TestCase):
    def test_staged_oracles_validate_goldens_and_reject_invalid_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "define-fixtures"
            stage.stage(destination)
            schemas = destination / "schemas"
            originals = ROOT / "rust/tests/fixtures/submission-schemas"
            for original in originals.rglob("*"):
                if original.is_file():
                    self.assertEqual(
                        (schemas / original.relative_to(originals)).read_bytes(),
                        original.read_bytes(),
                    )
            parser = etree.XMLParser(no_network=True, resolve_entities=False)
            xml_schema = etree.XMLSchema(
                etree.parse(str(schemas / "define/2.1/define2-1-0.xsd"), parser)
            )
            expected = destination / "sdtm-dm-metadata/expected"
            xml = etree.parse(str(expected / "define.xml"), parser)
            xml_schema.assertValid(xml)
            xml.getroot().set("FileType", "invalid")
            self.assertFalse(xml_schema.validate(xml))
            validator = Draft202012Validator(
                json.loads((schemas / "dataset.schema.json").read_bytes())
            )
            dataset = json.loads((expected / "dm.json").read_bytes())
            validator.validate(dataset)
            dataset["records"] = -1
            with self.assertRaises(ValidationError):
                validator.validate(dataset)


if __name__ == "__main__":
    unittest.main()
