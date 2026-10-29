"""Installed yamaa.schema(): the JSON Schema of a file type as canonical text.

The facade's own tests compare each schema with the file the repository
commits in yaml/json/ (the schema module of the yamaa crate); this checks
what the installed host returns and raises for the shared truth, which R reads
too (#3030 section 2.12, D149).
"""

import inspect
import json
import unittest

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "schema")


class Schema(unittest.TestCase):
    def test_schema_takes_one_file_type(self):
        self.assertEqual(
            list(inspect.signature(yamaa.schema).parameters), ["file_type"]
        )
        self.assertIn("schema", yamaa.__all__)
        self.assertEqual(
            [document["file_type"] for document in TRUTH["documents"]],
            TRUTH["file_types"],
        )

    def test_each_file_type_gives_its_json_schema_as_canonical_text(self):
        for document in TRUTH["documents"]:
            with self.subTest(file_type=document["file_type"]):
                text = yamaa.schema(document["file_type"])
                self.assertIsInstance(text, str)
                self.assertTrue(text.isascii())
                self.assertNotIn("\n", text)
                parsed = json.loads(text)
                # Canonical: members in key order, no insignificant whitespace.
                self.assertEqual(
                    json.dumps(parsed, sort_keys=True, separators=(",", ":")), text
                )
                self.assertEqual(list(parsed), TRUTH["keys"])
                self.assertEqual(parsed["$schema"], TRUTH["dialect"])
                self.assertEqual(parsed["title"], document["title"])
                self.assertEqual(parsed["$ref"], document["ref"])
                self.assertEqual(yamaa.schema(document["file_type"]), text)

    def test_any_other_text_raises_the_refusal_with_its_issue(self):
        for refusal in TRUTH["refusals"]:
            with self.subTest(file_type=refusal["file_type"]):
                with self.assertRaises(yamaa.Error) as caught:
                    yamaa.schema(refusal["file_type"])
                error = caught.exception
                self.assertEqual(str(error), refusal["message"])
                self.assertEqual(error.paths, [])
                [issue] = error.issues.to_pylist()
                self.assertEqual(issue["condition"], refusal["condition"])
                self.assertEqual(issue["rule"], refusal["rule"])
                self.assertEqual(issue["message"], refusal["issue_message"])
                self.assertEqual(issue["context"], refusal["context"])
                self.assertEqual(issue["spec_paths"], [])

    def test_an_argument_that_is_not_text_is_a_package_error(self):
        for value in (None, 1, b"spec", ["spec"]):
            with self.subTest(value=value), self.assertRaises(yamaa.Error):
                yamaa.schema(value)


if __name__ == "__main__":
    unittest.main()
