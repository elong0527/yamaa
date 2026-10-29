"""The generated interface handshake of both hosts and the native gate check."""

import copy
import re
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import bindings_handshake as handshake
from tools.packaging import generate_bindings as generator


class HandshakeTests(unittest.TestCase):
    def setUp(self):
        self.definition = generator._load_definition()

    def test_both_requests_carry_the_boundary_version_of_yamaa_boundary(self):
        # The version is read from the boundary crate, never kept as a copy.
        source = handshake.BOUNDARY.read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as directory:
            boundary = Path(directory) / "lib.rs"
            boundary.write_text(
                re.sub(
                    r"pub const BOUNDARY_VERSION: u32 = [0-9]+;",
                    "pub const BOUNDARY_VERSION: u32 = 7;",
                    source,
                ),
                encoding="utf-8",
            )
            with mock.patch.object(handshake, "BOUNDARY", boundary):
                python = handshake.python_lines(self.definition)
                r = handshake.r_lines(self.definition)
        python = python[python.index("_INTERFACE_REQUEST = {") :][:5]
        r = r[r.index(".interface_request <- list(") :][:5]
        self.assertEqual(
            {"python": python, "r": r},
            {
                "python": [
                    "_INTERFACE_REQUEST = {",
                    '    "version": 7,',
                    '    "required": ["structured_records", "arrow_c_stream", "plan_reuse"],',
                    '    "optional": ["format_yaml", "format_parquet", "format_readstat", "format_odm"],',
                    "}",
                ],
                "r": [
                    ".interface_request <- list(",
                    "  version = 7L,",
                    '  required = c("structured_records", "arrow_c_stream", "plan_reuse"),',
                    '  optional = c("format_yaml", "format_parquet", "format_readstat", "format_odm")',
                    ")",
                ],
            },
            "each host's request asks for yamaa-boundary's version 7",
        )

    def test_the_handshake_section_is_a_closed_grammar(self):
        def section(edit):
            def mutate(definition):
                edit(definition["handshake"])

            return mutate

        mutations = [
            ("must hold exactly", section(lambda h: h.update(failure_record="x"))),
            ("boundary.version", section(lambda h: h.update(version="records.x"))),
            ("no python function", section(lambda h: h.update(python="_absent"))),
            ("no r function", section(lambda h: h.update(r="absent"))),
            ("census must name", section(lambda h: h["census"].pop("r"))),
            (
                "_absent, no python",
                section(lambda h: h["census"].update(python="_absent")),
            ),
            ("python_error", section(lambda h: h.update(python_error="Interface"))),
            ("no identifier", section(lambda h: h["required_features"].append("x"))),
            (
                "yamaa-boundary's formats",
                section(lambda h: h["optional_features"].pop("format_odm")),
            ),
            (
                "an identifier twice",
                section(lambda h: h["required_features"].append("format_odm")),
            ),
            (
                "messages must be",
                section(lambda h: h["messages"].pop("missing_feature")),
            ),
            (
                "may name only",
                section(
                    lambda h: h["messages"].update(
                        malformed_request="needs {requested_version}"
                    )
                ),
            ),
            (
                "quote or backslash",
                section(lambda h: h["messages"].update(missing_interface='a "b"')),
            ),
        ]
        for message, mutate in mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(self.definition)
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    handshake.validate(candidate)

    def test_the_imports_resolve_to_their_owners(self):
        mutations = [
            ("boundary roles", lambda i: i["boundary"].update(roles=["version", "x"])),
            ("yamaa-boundary crate", lambda i: i["boundary"].update(crate="yamaa")),
            ("missing the records import", lambda i: i.pop("records")),
        ]
        for message, mutate in mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(self.definition)
                mutate(candidate["imports"])
                with self.assertRaisesRegex(ValueError, message):
                    generator._resolve_imports(candidate)

    def test_every_registered_native_function_must_reach_the_gate(self):
        # The repository passes; an export whose body skips the gate fails.
        generator._check_gate(self.definition)
        read_text = Path.read_text
        python = generator.RUST_TARGET.parent / "api_entry.rs"
        r = generator.R_SOURCE / "api_entry.rs"
        # Each binding's helper that passes the gate, swapped for a name that
        # none of the binding's functions has.
        cases = [
            (
                python,
                "pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {\n"
                "    plain(py,",
                "pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {\n"
                "    ungated(py,",
                "python native functions that do not reach the interface gate: _schema",
            ),
            (
                r,
                "fn schema(file_type: Raw) -> List {\n    reply(",
                "fn schema(file_type: Raw) -> List {\n    ungated(",
                "r native functions that do not reach the interface gate: schema",
            ),
        ]

        def skipping(path, old, new):
            """Path.read_text, with `old` replaced by `new` in `path`."""

            def read(source, *arguments, **keywords):
                text = read_text(source, *arguments, **keywords)
                if source != path:
                    return text
                self.assertEqual(text.count(old), 1)
                return text.replace(old, new)

            return read

        for path, old, new, message in cases:
            with self.subTest(path=path.name, message=message):
                with (
                    mock.patch.object(Path, "read_text", skipping(path, old, new)),
                    self.assertRaisesRegex(ValueError, message),
                ):
                    generator._check_gate(self.definition)

    def test_only_the_informational_version_is_exempt_from_the_gate(self):
        # The R function of an operation whose Python side is the version
        # constant needs no gate; an operation with no Python side exempts
        # nothing.
        candidate = copy.deepcopy(self.definition)
        candidate["operations"]["core_version"]["python"] = []
        with self.assertRaisesRegex(
            ValueError,
            "r native functions that do not reach the interface gate: core_version",
        ):
            generator._check_gate(candidate)

    def test_each_load_negotiates_before_anything_else(self):
        # Python imports nothing else from the package before the handshake,
        # and R's .onLoad does nothing but negotiate.
        rendered = generator.render_all()
        module = rendered[generator.PUBLIC_PYTHON_TARGET].splitlines()
        negotiation = module.index("_negotiate(_native)")
        imports = [
            (index < negotiation, line)
            for index, line in enumerate(module)
            if re.match(r"from [\w.]+ import ", line)
        ]
        wrappers = rendered[generator.R_WRAPPERS_TARGET].splitlines()
        load = wrappers[wrappers.index(".onLoad <- function(libname, pkgname) {") :]
        self.assertEqual(
            {"python imports, before the negotiation or not": imports, "R load": load},
            {
                "python imports, before the negotiation or not": [
                    (True, "from yamaa import _native"),
                    (
                        False,
                        "from yamaa._native import Error, StateError, Table  # noqa: E402",
                    ),
                    # With its reason, the helpers' import is too wide for one
                    # line, so it is split, the reason on its first line.
                    (False, "from yamaa._support import (  # noqa: E402"),
                ],
                "R load": [
                    ".onLoad <- function(libname, pkgname) {",
                    "  namespace <- topenv()",
                    "  .interface_negotiate(function(name) {",
                    '    if (exists(paste0("wrap__", name), envir = namespace, '
                    "inherits = FALSE)) {",
                    '      get(paste0(".native_", name), envir = namespace, '
                    "inherits = FALSE)",
                    "    }",
                    "  })",
                    "}",
                ],
            },
            "each load negotiates before anything else",
        )


if __name__ == "__main__":
    unittest.main()
