"""Keep the package-owned schema synchronized without content hashes."""

import unittest

from tools.packaging import generate_shipped_schema as generator


class ShippedSchemaTests(unittest.TestCase):
    def test_generated_source_matches_authoritative_bytes(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("ascii")
        )


if __name__ == "__main__":
    unittest.main()
