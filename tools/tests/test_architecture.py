"""The architecture check refuses each crate edge and each mark of file or
format code in core that #3030 section 2.10 rules out."""

import contextlib
import io
import tempfile
import textwrap
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.tests.support import write
from tools.validation import check_architecture as architecture

MEMBERS = {
    "yamaa-core": (),
    "yamaa-adapters": ("yamaa-core",),
    "yamaa-application": ("yamaa-adapters", "yamaa-core"),
    "yamaa": ("yamaa-application", "yamaa-adapters", "yamaa-core"),
    "yamaa-boundary": (),
    "yamaa-python": ("yamaa", "yamaa-boundary"),
    "yamaa-r": ("yamaa", "yamaa-boundary"),
    "yamaa-cli": ("yamaa",),
}
CORE = "rust/crates/yamaa-core"


def manifest(name, dependencies, extra=""):
    lines = [f'[package]\nname = "{name}"\nversion = "0.1.0"\n\n[dependencies]']
    lines += [f'{crate} = {{ path = "../{crate}" }}' for crate in dependencies]
    return "\n".join(lines) + "\n" + extra


class ArchitectureTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        members = ", ".join(f'"crates/{name}"' for name in MEMBERS)
        self.write(
            "rust/Cargo.toml",
            f'[workspace]\nexclude = ["vendor/parser"]\nmembers = [{members}]\n',
        )
        for name, dependencies in MEMBERS.items():
            self.write(f"rust/crates/{name}/Cargo.toml", manifest(name, dependencies))
        # A vendored crate outside the workspace, as the YAML parser is.
        self.write("rust/vendor/parser/Cargo.toml", manifest("yamaa-parser", ()))
        self.write(f"{CORE}/src/lib.rs", "pub mod value;\n")
        self.write(
            f"{CORE}/src/value.rs", "pub fn add(a: i64, b: i64) -> i64 { a + b }\n"
        )

    def write(self, relative, text):
        return write(self.root, relative, textwrap.dedent(text))

    def append(self, relative, text):
        path = self.root / relative
        path.write_text(path.read_text(encoding="utf-8") + text, encoding="utf-8")

    def errors(self):
        return architecture.check(self.root)

    def test_the_repository_passes(self):
        self.assertEqual(architecture.check(ROOT), [])

    def test_a_workspace_that_keeps_section_2_10_passes(self):
        self.assertEqual(self.errors(), [])

    # -- crate edges ----------------------------------------------------------

    def test_core_depends_on_nothing_else_in_the_workspace(self):
        self.append(
            f"{CORE}/Cargo.toml",
            '\n[dev-dependencies]\nyamaa-adapters = { path = "../yamaa-adapters" }\n',
        )
        self.assertEqual(
            self.errors(),
            [
                f"{CORE}/Cargo.toml: [dev-dependencies] yamaa-core depends on "
                "yamaa-adapters, but core depends on nothing else in the workspace "
                "(#3030 section 2.10)"
            ],
        )

    def test_each_layer_depends_only_on_the_layers_below_it(self):
        cases = [
            (
                "yamaa-adapters",
                '\n[build-dependencies]\nyamaa = { path = "../yamaa" }\n',
                "[build-dependencies] yamaa-adapters depends on yamaa, but the "
                "adapters depend on core alone",
            ),
            (
                "yamaa-adapters",
                "\n[dev-dependencies]\n"
                'yamaa-application = { path = "../yamaa-application" }\n',
                "[dev-dependencies] yamaa-adapters depends on yamaa-application, "
                "but the adapters depend on core alone",
            ),
            (
                "yamaa-application",
                '\n[dev-dependencies]\nyamaa = { path = "../yamaa" }\n',
                "[dev-dependencies] yamaa-application depends on yamaa, but the "
                "application layer depends on the adapters and core alone",
            ),
            (
                "yamaa",
                '\n[dev-dependencies]\nyamaa-cli = { path = "../yamaa-cli" }\n',
                "[dev-dependencies] yamaa depends on yamaa-cli, but the facade "
                "depends on the application, the adapters and core alone",
            ),
            (
                "yamaa-boundary",
                '\n[dev-dependencies]\nyamaa = { path = "../yamaa" }\n',
                "[dev-dependencies] yamaa-boundary depends on yamaa, but the "
                "boundary depends on nothing else in the workspace",
            ),
            (
                "yamaa-python",
                '\n[target."cfg(unix)".dependencies]\n'
                'yamaa-core = { path = "../yamaa-core" }\n',
                "[target.cfg(unix).dependencies] yamaa-python depends on "
                "yamaa-core, but a host binding depends on the facade and the "
                "boundary alone",
            ),
            (
                "yamaa-r",
                "\n[dev_dependencies]\n"
                'engine = { package = "yamaa-adapters", path = "../yamaa-adapters" }\n',
                "[dev_dependencies] yamaa-r depends on yamaa-adapters, but a host "
                "binding depends on the facade and the boundary alone",
            ),
            (
                "yamaa-cli",
                '\n[dev-dependencies]\nyamaa-r = { path = "../yamaa-r" }\n',
                "[dev-dependencies] yamaa-cli depends on yamaa-r, but a host "
                "depends on the facade alone",
            ),
            (
                "yamaa-cli",
                '\n[dependencies.yamaa-boundary]\npath = "../yamaa-boundary"\n',
                "[dependencies] yamaa-cli depends on yamaa-boundary, but a host "
                "depends on the facade alone",
            ),
        ]
        for crate, table, message in cases:
            with self.subTest(crate=crate, message=message):
                path = f"rust/crates/{crate}/Cargo.toml"
                original = (self.root / path).read_text(encoding="utf-8")
                self.append(path, table)
                self.assertEqual(
                    self.errors(),
                    [f"{path}: {message} (#3030 section 2.10)"],
                )
                self.write(path, original)

    def test_a_dependency_inherited_from_the_workspace_keeps_the_edges(self):
        self.append(
            "rust/Cargo.toml",
            "\n[workspace.dependencies]\n"
            'engine = { package = "yamaa-core", path = "crates/yamaa-core" }\n'
            'yamaa-adapters = { path = "crates/yamaa-adapters" }\n'
            'facade = { package = "yamaa", path = "crates/yamaa" }\n'
            'thiserror = "2"\n',
        )
        path = "rust/crates/yamaa-python/Cargo.toml"
        original = (self.root / path).read_text(encoding="utf-8")
        refusal = (
            "{table} yamaa-python depends on {package}, but a host binding depends "
            "on the facade and the boundary alone (#3030 section 2.10)"
        )
        cases = {
            "engine = { workspace = true }\n": refusal.format(
                table="[dependencies]", package="yamaa-core"
            ),
            "\n[build-dependencies]\nengine = { workspace = true }\n": refusal.format(
                table="[build-dependencies]", package="yamaa-core"
            ),
            "\n[dev-dependencies]\nyamaa-adapters = { workspace = true }\n": (
                refusal.format(table="[dev-dependencies]", package="yamaa-adapters")
            ),
            '\n[target."cfg(windows)".dependencies]\n'
            "engine = { workspace = true, optional = true }\n": refusal.format(
                table="[target.cfg(windows).dependencies]", package="yamaa-core"
            ),
        }
        for table, message in cases.items():
            with self.subTest(table=table):
                self.append(path, table)
                self.assertEqual(self.errors(), [f"{path}: {message}"])
                self.write(path, original)
        # The facade under an inherited rename, and a crate outside the
        # workspace, are allowed.
        self.append(path, "facade = { workspace = true }\nthiserror.workspace = true\n")
        self.assertEqual(self.errors(), [])
        self.write(path, original)
        self.append(path, "ghost = { workspace = true }\n")
        self.assertEqual(
            self.errors(),
            [
                f"{path}: cannot read the dependency tables: [dependencies] ghost "
                "inherits no [workspace.dependencies] entry"
            ],
        )

    def test_crates_outside_the_workspace_are_outside_the_rule(self):
        self.append(
            f"{CORE}/Cargo.toml",
            'yamaa-parser = { path = "../../vendor/parser" }\nthiserror = "2"\n',
        )
        self.assertEqual(self.errors(), [])

    def test_every_workspace_member_has_a_layer(self):
        self.write("rust/crates/yamaa-extra/Cargo.toml", manifest("yamaa-extra", ()))
        workspace = self.root / "rust/Cargo.toml"
        workspace.write_text(
            workspace.read_text(encoding="utf-8")
            .replace('"crates/yamaa-r", ', "")
            .replace('"crates/yamaa-cli"', '"crates/yamaa-cli", "crates/yamaa-extra"'),
            encoding="utf-8",
        )
        self.assertEqual(
            self.errors(),
            [
                f"{architecture.SELF}: LAYER names yamaa-r, which is not a "
                "workspace member",
                "rust/crates/yamaa-extra/Cargo.toml: yamaa-extra has no layer of "
                f"#3030 section 2.10; give it one in {architecture.SELF}'s LAYER",
            ],
        )

    def test_an_unreadable_workspace_fails(self):
        for text in (
            "[workspace\n",
            "[package]\nname = 'x'\n",
            '[workspace]\nmembers = ["crates/none"]\n',
        ):
            with self.subTest(text=text):
                self.write("rust/Cargo.toml", text)
                errors = self.errors()
                self.assertEqual(len(errors), 1, errors)
                self.assertTrue(
                    errors[0].startswith(
                        "rust/Cargo.toml: cannot read the workspace: "
                    ),
                    errors,
                )

    def test_a_malformed_dependency_table_fails(self):
        self.write(
            "rust/crates/yamaa-r/Cargo.toml",
            '[package]\nname = "yamaa-r"\nversion = "0.1.0"\n\n[target]\nunix = 1\n',
        )
        errors = self.errors()
        self.assertEqual(len(errors), 1, errors)
        self.assertTrue(
            errors[0].startswith(
                "rust/crates/yamaa-r/Cargo.toml: cannot read the dependency tables: "
            ),
            errors,
        )

    # -- core sources ---------------------------------------------------------

    def test_file_code_is_found_through_imports_aliases_and_full_paths(self):
        cases = {
            "use std::fs;\n": [(1, "file", "std::fs")],
            "use std::{fs, io::Write as _};\n": [
                (1, "file", "std::fs"),
                (1, "file", "std::io"),
            ],
            'fn f() {\n    let _ = ::std::net::TcpStream::connect("x");\n}\n': [
                (2, "file", "std::net")
            ],
            "use std::time;\nfn f() {\n    time::SystemTime::now();\n}\n": [
                (3, "file", "std::time::SystemTime")
            ],
            'use std as host;\nfn f() {\n    host::env::var("X");\n}\n': [
                (3, "file", "std::env")
            ],
            "fn f() -> std::path::PathBuf {\n    todo!()\n}\n": [
                (1, "file", "std::path")
            ],
            "use std::os::unix::fs::PermissionsExt;\nuse std::process::Command;\n": [
                (1, "file", "std::os"),
                (2, "file", "std::process"),
            ],
        }
        for source, expected in cases.items():
            with self.subTest(source=source):
                self.assertEqual(architecture.marks(source), expected)

    def test_format_code_decodes_bytes_or_writes_a_format_mark(self):
        cases = {
            "fn f(b: &[u8]) {\n    let _ = std::str::from_utf8(b);\n}\n": [
                (2, "format", "from_utf8 decodes bytes as text")
            ],
            "fn f(b: Vec<u8>) {\n    let _ = String::from_utf8_lossy(&b);\n}\n": [
                (2, "format", "from_utf8_lossy decodes bytes as text")
            ],
            'const HEAD: &str = "<?xml version=\\"1.0\\"?>";\n': [
                (1, "format", "'<?xml', an XML declaration")
            ],
            'fn f(text: &str) -> String {\n    text.replace("&", "&amp;")\n}\n': [
                (2, "format", "'&amp;', an XML escape")
            ],
            'const MAGIC: &[u8] = b"PAR1";\n': [
                (1, "format", "'PAR1', the Parquet magic number")
            ],
            'const LIBRARY: &str = r"HEADER RECORD*******LIBRARY HEADER RECORD";\n': [
                (1, "format", "'HEADER RECORD', a SAS transport header record")
            ],
        }
        for source, expected in cases.items():
            with self.subTest(source=source):
                self.assertEqual(architecture.marks(source), expected)

    def test_comments_text_test_items_and_other_modules_hold_no_marks(self):
        for source in (
            "// use std::fs;\n/// Reads with std::fs and from_utf8, writes <?xml.\n",
            'const NOTE: &str = "std::fs::read";\n',
            "use std::fmt::Write;\nuse std::time::Duration;\nuse core::ops::Add;\n",
            "mod io {\n    pub fn f() {}\n}\nfn g() {\n    io::f();\n}\n",
            "fn f() {\n    let path = 1;\n    let fs = path;\n}\n",
            textwrap.dedent(
                """\
                #[cfg(test)]
                mod tests {
                    use std::fs;
                    #[test]
                    fn reads() {
                        let _ = std::str::from_utf8(b"x");
                        let _ = "<?xml";
                    }
                }
                """
            ),
        ):
            with self.subTest(source=source):
                self.assertEqual(architecture.marks(source), [])

    def test_core_file_and_format_code_fails_at_its_line(self):
        self.write(
            f"{CORE}/src/reader.rs",
            """\
            //! Reads a source.
            use std::io::Read;

            pub fn text(bytes: &[u8]) -> &str {
                std::str::from_utf8(bytes).unwrap()
            }
            """,
        )
        self.assertEqual(
            self.errors(),
            [
                f"{CORE}/src/reader.rs:2: file code in core: std::io",
                f"{CORE}/src/reader.rs:5: format code in core: from_utf8 decodes "
                "bytes as text",
            ],
        )

    def test_core_without_sources_fails(self):
        for path in (self.root / CORE / "src").iterdir():
            path.unlink()
        self.assertEqual(self.errors(), [f"{CORE}/src: no core sources found"])

    # -- the command line -----------------------------------------------------

    def test_the_command_line_passes_and_fails_with_a_hint(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(architecture.main(["--root", str(self.root)]), 0)
            self.assertIn("core holds no file or format code.", output.getvalue())
            self.write(f"{CORE}/src/value.rs", "use std::fs;\n")
            self.assertEqual(architecture.main(["--root", str(self.root)]), 1)
        lines = output.getvalue().splitlines()
        self.assertIn(
            f"ERROR: {CORE}/src/value.rs:1: file code in core: std::fs", lines
        )
        self.assertEqual(lines[-1], architecture.HINT)


if __name__ == "__main__":
    unittest.main()
