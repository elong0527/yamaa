"""Keep the committed facade inventory synchronized with the facade crate."""

import unittest

from tools.packaging import generate_facade_api as generator


class FacadeApiTests(unittest.TestCase):
    def test_committed_inventory_matches_facade_sources(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("utf-8")
        )


if __name__ == "__main__":
    unittest.main()
