import unittest
from pathlib import Path

from yamaa.portable_registry import PortableRegistry, run_conformance


ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "yaml/registry/portable-functions.yaml"
FIXTURES = ROOT / "yaml/registry/conformance.yaml"


class PortableRegistryTest(unittest.TestCase):
    def test_core_registry_is_valid(self):
        registry = PortableRegistry.load(REGISTRY)
        self.assertEqual(registry.core["registry_version"], "1.0.0")
        self.assertEqual(registry.core["namespace"], "core")

    def test_shared_conformance_fixtures(self):
        evaluation_count, validation_count = run_conformance(REGISTRY, FIXTURES)
        self.assertGreaterEqual(evaluation_count, 19)
        self.assertGreaterEqual(validation_count, 7)


if __name__ == "__main__":
    unittest.main()
