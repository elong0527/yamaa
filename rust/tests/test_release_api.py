"""A release inventory must notice export drift without importing reference code."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('release_api', Path(__file__).parents[1] / 'tools/check_release_api.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReleaseApiInventory(unittest.TestCase):
    def test_actual_checkout_has_explicit_dispositions(self):
        root = Path(__file__).resolve().parents[2]
        module.check(root, json.loads((root / 'rust/planning/release-api.json').read_text()))

    def test_inspection_never_imports_the_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / 'python/src/yamaa/__init__.py'
            path.parent.mkdir(parents=True)
            path.write_text('raise RuntimeError("must not import")\n__all__ = ["domain"]\n')
            self.assertEqual(module.discover(root), {'python/src/yamaa/__init__.py': ['domain']})

    def test_dynamic_and_duplicate_exports_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / 'python/src/yamaa/__init__.py'
            path.parent.mkdir(parents=True)
            for source in ['__all__ = calculate_exports()', '__all__ = ["a", "a"]', '__all__ = []\n__all__ += ["b"]', '__all__ = []\n__all__.append("b")', 'if True:\n    __all__ = ["b"]']:
                with self.subTest(source=source):
                    path.write_text(source)
                    with self.assertRaises(ValueError):
                        module.discover(root)

    def test_changed_names_and_unregistered_sources_do_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / 'python/src/yamaa/__init__.py'
            path.parent.mkdir(parents=True)
            path.write_text('__all__ = ["domain"]\n')
            inventory = {'version': 1, 'sources': {'python/src/yamaa/__init__.py': {'names': ['domain'], 'disposition': 'replace', 'issue': '#1751', 'rationale': 'qualify replacement', 'overrides': {}}}}
            module.check(root, inventory)
            path.write_text('__all__ = ["domain", "check"]\n')
            with self.assertRaisesRegex(ValueError, 'declarations differ'):
                module.check(root, inventory)
            path.write_text('__all__ = ["domain"]\n')
            namespace = root / 'R/yamaa/NAMESPACE'
            namespace.parent.mkdir(parents=True)
            namespace.write_text('export(yamaa_domain)\n')
            with self.assertRaisesRegex(ValueError, 'sources differ'):
                module.check(root, inventory)

    def test_namespace_and_stub_members_are_visible(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            namespace = root / 'R/yamaa/NAMESPACE'
            namespace.parent.mkdir(parents=True)
            namespace.write_text('export(yamaa_domain)\nS3method(as.character,yamaa_int64)\nS3method("[",yamaa_int64_vector)\n')
            stub = root / 'python/src/yamaa/_native.pyi'
            stub.parent.mkdir(parents=True)
            stub.write_text('VERSION: str\nDEFAULT = ...\nclass _Result:\n    ok: bool\n    def save(self): ...\nasync def inspect(): ...\n')
            actual = module.discover(root)
            self.assertEqual(actual[namespace.relative_to(root).as_posix()], ['S3:[.yamaa_int64_vector', 'S3:as.character.yamaa_int64', 'yamaa_domain'])
            self.assertEqual(actual[stub.relative_to(root).as_posix()], ['DEFAULT', 'VERSION', '_Result', '_Result.ok', '_Result.save', 'inspect'])
            stub.write_text('if True:\n    def conditional(): ...\n')
            with self.assertRaisesRegex(ValueError, 'conditional stub declarations'):
                module.discover(root)


if __name__ == '__main__':
    unittest.main()
