"""Keep the package-owned schema synchronized without content hashes."""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "generate_shipped_schema",
    Path(__file__).parents[1] / "tools/generate_shipped_schema.py",
)
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class ShippedSchemaTests(unittest.TestCase):
    def test_generated_source_matches_authoritative_bytes(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("ascii")
        )


if __name__ == "__main__":
    unittest.main()
