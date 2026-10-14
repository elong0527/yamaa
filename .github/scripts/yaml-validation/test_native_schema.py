"""Independent boundary controls for read-only native schema facts and IO."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import _native_schema as SCHEMA
import _native_specification as SPECIFICATION
from yamaa._native import _capture_schema_metadata


class TestNativeSchemaFacts(unittest.TestCase):
    def schema(self, descriptor):
        return _capture_schema_metadata([('schema.yaml', SCHEMA.bytes_for({
            'version': '1.0', 'root_class': [{'X': descriptor}],
        }))], 0, 'root_class')

    def test_unicode_and_nul_round_trip(self):
        authored = {'X': '\u00e9\U0001f642\x00'}
        encoded = SCHEMA.bytes_for(authored)
        self.assertTrue(encoded.isascii())
        schema = self.schema({'type': 'str'})
        self.assertEqual(schema.validate(encoded, ['root_class'], False), [])
        self.assertEqual(SCHEMA.decode_document(schema.normalize(
            encoded, ['root_class'], False)), authored)

    def test_wide_radix_integer_has_exact_lossless_local_codec(self):
        schema = self.schema({'type': 'int'})
        authored = b'X: 0x' + b'f' * 3600 + b'\n'
        decoded = SCHEMA.decode_document(schema.normalize(
            authored, ['root_class'], False))
        self.assertEqual(decoded['X'], (1 << 14400) - 1)
        self.assertIs(type(decoded['X']), int)
        self.assertEqual(SCHEMA.decode_document(schema.normalize(
            SCHEMA.bytes_for(decoded), ['root_class'], False)), decoded)

    def test_scalar_kind_and_signed_bounds_remain_exact(self):
        for kind, value in [('int', -(1 << 63)), ('int', (1 << 63) - 1),
                ('bool', False), ('bool', True)]:
            schema = self.schema({'type': kind})
            decoded = SCHEMA.decode_document(schema.normalize(
                SCHEMA.bytes_for({'X': value}), ['root_class'], False))['X']
            self.assertIs(type(decoded), type(value))
            self.assertEqual(decoded, value)

    def test_standalone_core_yaml_names_do_not_use_yaml11_resolution(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / 'schema.yaml').write_text(
                'version: "1.0"\nroot_class: [{FLAG: {type: on}}]\n'
                'on: {type: str}\nyes: {type: str}\n')
            schema = SCHEMA.capture(directory)
            self.assertEqual(schema.validate(b'FLAG: hello\n', ['root_class'], False), [])

    def test_symlink_include_fails_before_outside_bytes_are_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / 'inside'
            directory.mkdir()
            outside = Path(temporary) / 'outside.yaml'
            outside.write_text('root_class: []\n')
            (directory / 'schema.yaml').write_text('version: "1.0"\nincludes: [schema_child.yaml]\n')
            (directory / 'schema_child.yaml').symlink_to(outside)
            with self.assertRaisesRegex(OSError, 'symbolic link'):
                SCHEMA.capture(directory)

    def test_permission_failure_keeps_original_cause(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / 'schema.yaml').write_text('version: "1.0"\nincludes: [schema_child.yaml]\n')
            (directory / 'schema_child.yaml').write_text('root_class: []\n')
            original = Path.open
            def opened(path, *args, **kwargs):
                if path.name == 'schema_child.yaml':
                    raise PermissionError('authored physical refusal')
                return original(path, *args, **kwargs)
            with patch.object(Path, 'open', opened), self.assertRaisesRegex(
                    PermissionError, 'authored physical refusal'):
                SCHEMA.capture(directory)

    def test_missing_include_is_native_admission_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / 'schema.yaml').write_text('version: "1.0"\nincludes: [schema_child.yaml]\nroot_class: []\n')
            with self.assertRaisesRegex(ValueError, 'MissingInclude'):
                SCHEMA.capture(directory)

    def test_query_carrier_bound_refuses(self):
        schema = self.schema({'type': 'str'})
        with self.assertRaises(ValueError):
            schema.validate(b'X: a\n', ['str'] * 129, False)
        with self.assertRaises(ValueError):
            schema.normalize(b'X: a\n', ['x' * 65_537], False)

    def test_failed_reused_request_clears_successful_held_document(self):
        schema = self.schema({'type': 'str'})
        class Owner:
            def check_issues(self): return []
            def preparation_status(self): return 'ready'
            def resolved_document(self): return schema.normalize(b'X: held\n', ['root_class'], False)
        metadata = {}
        with patch.object(SPECIFICATION, 'prepare', return_value=Owner()):
            self.assertEqual(SPECIFICATION.issue_rows(Path('first.yaml'), metadata=metadata,
                document_only=True), [])
        self.assertEqual(metadata['document'], {'X': 'held'})
        with patch.object(SPECIFICATION, 'prepare', side_effect=RuntimeError('transport unavailable')), \
                self.assertRaisesRegex(RuntimeError, 'transport unavailable'):
            SPECIFICATION.issue_rows(Path('second.yaml'), metadata=metadata, document_only=True)
        self.assertEqual(metadata, {})


if __name__ == '__main__':
    unittest.main()
