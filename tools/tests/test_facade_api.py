"""Keep the committed facade inventory synchronized with the facade crate."""

import unittest

from tools.packaging import generate_facade_api as generator


class FacadeApiTests(unittest.TestCase):
    def test_committed_inventory_matches_facade_sources(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("utf-8")
        )

    def test_only_exact_test_gates_are_test_only(self):
        self.assertTrue(generator._is_test_cfg(["#[cfg(test)]"]))
        self.assertTrue(generator._is_test_cfg(['#[cfg(all(test, unix))]']))
        self.assertFalse(generator._is_test_cfg(["#[cfg(not(test))]"]))
        self.assertFalse(generator._is_test_cfg(['#[cfg(feature = "test-utils")]']))
        self.assertFalse(generator._is_test_cfg(["#[cfg(unix)]"]))
        self.assertFalse(generator._is_test_cfg([]))


if __name__ == "__main__":
    unittest.main()
