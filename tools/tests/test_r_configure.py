"""The R package's configure step: find, check and report Cargo and rustc.

These tests run R/yamaa/tools/configure.R, which configure and configure.win
run, against fake cargo and rustc commands. They need R; `make test-r` runs
them.
"""

import os
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

from tools.ci import toolchain
from tools.paths import ROOT

PACKAGE = ROOT / "R/yamaa"
RSCRIPT = shutil.which("Rscript")
SYSTEM_PATH = "/usr/bin:/bin"


def tool(directory, name, line, status=0, sysroot=None):
    """Write a fake command that prints `line` for --version.

    Like a rustup proxy that may install a toolchain, it fails unless
    RUSTUP_AUTO_INSTALL=0. `--print sysroot` prints `sysroot`.
    """
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text(
        textwrap.dedent(
            f"""\
            #!/bin/sh
            [ "$RUSTUP_AUTO_INSTALL" = 0 ] || {{ echo "would install" >&2; exit 3; }}
            case "$1" in
              --version) echo "{line}"; exit {status} ;;
              --print) echo "{sysroot}" ;;
            esac
            """
        ),
        encoding="utf-8",
    )
    path.chmod(0o755)
    return path


@unittest.skipIf(RSCRIPT is None, "R is not installed")
@unittest.skipIf(
    shutil.which("cargo", path=SYSTEM_PATH) or shutil.which("rustc", path=SYSTEM_PATH),
    "a system Cargo or rustc would be found",
)
class ConfigureTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.package = self.root / "yamaa"
        for relative in ("tools/configure.R", "src/Makevars.in", "src/Makevars.win.in"):
            (self.package / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(PACKAGE / relative, self.package / relative)
        self.manifest(toolchain.minimum())
        self.bin = self.root / "bin"
        self.home = self.root / "home"
        self.home.mkdir()

    def manifest(self, version):
        manifest = self.package / "src/rust/Cargo.toml"
        manifest.parent.mkdir(parents=True, exist_ok=True)
        manifest.write_text(
            f'[workspace]\nmembers = []\n\n[workspace.package]\nrust-version = "{version}"\n',
            encoding="utf-8",
        )

    def configure(self, expression=None, **environment):
        """Run configure.R in the package, or an expression after sourcing it."""
        arguments = (
            ["tools/configure.R"]
            if expression is None
            else ["-e", f'source("tools/configure.R"); {expression}']
        )
        return subprocess.run(
            [RSCRIPT, "--vanilla", *arguments],
            cwd=self.package,
            env={
                "PATH": f"{self.bin}:{SYSTEM_PATH}",
                "HOME": str(self.home),
                "LANG": "C.UTF-8",
                **environment,
            },
            capture_output=True,
            text=True,
        )

    def makevars(self, name="Makevars"):
        return (self.package / "src" / name).read_text(encoding="utf-8")

    def assert_refused(self, result, *phrases):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        for phrase in phrases:
            self.assertIn(phrase, result.stderr)
        self.assertFalse((self.package / "src/Makevars").exists())

    def test_tools_on_the_path_are_reported_and_written_into_makevars(self):
        rustc = tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        cargo = tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        result = self.configure()
        self.assertEqual(result.returncode, 0, result.stderr)
        # R CMD check needs the rustc version in the installation log before
        # compilation starts.
        self.assertEqual(
            result.stdout.splitlines(),
            [
                f"using rustc 1.99.1 (58cf8f9c3 2026-10-13) from {rustc}",
                f"using cargo 1.99.1 (5f94df478 2026-08-27) from {cargo}",
            ],
        )
        self.assertRegex(result.stdout, r"rustc *[0-9]+[.][0-9]")
        makevars = self.makevars()
        self.assertIn(f"\nCARGO = {cargo}\n", makevars)
        self.assertIn(f"\nRUSTC = {rustc}\n", makevars)
        self.assertIn("\nCARGO_ENV = \n", makevars)
        self.assertNotIn("@", makevars)
        template = (PACKAGE / "src/Makevars.in").read_text(encoding="utf-8")
        self.assertEqual(len(makevars.splitlines()), len(template.splitlines()))
        self.assertFalse((self.package / "src/Makevars.win").exists())

    def test_personal_installations_are_found_off_the_path(self):
        for environment, directory in (
            ({}, self.home / ".cargo/bin"),
            ({"CARGO_HOME": str(self.root / "cargo")}, self.root / "cargo/bin"),
        ):
            with self.subTest(directory=directory):
                rustc = tool(directory, "rustc", "rustc 1.90.0 (1159e78c4 2025-09-14)")
                cargo = tool(directory, "cargo", "cargo 1.90.0 (840b83a10 2025-07-30)")
                result = self.configure(**environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"from {rustc}\n", result.stdout)
                self.assertIn(f"\nCARGO = {cargo}\n", self.makevars())

    def test_missing_tools_ask_for_an_installation(self):
        result = self.configure()
        self.assert_refused(
            result,
            f"needs Cargo and rustc {toolchain.minimum()} or later",
            "No 'rustc' command was found on the PATH or in "
            f"{self.home / '.cargo/bin'}",
            "apt-get install cargo rustc",
            "dnf install cargo",
            "https://rustup.rs/",
        )
        tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        self.assert_refused(self.configure(), "No 'cargo' command was found")

    def test_older_tools_than_the_workspace_rust_version_are_refused(self):
        for rustc, cargo, refused in (
            ("1.89.0", "1.99.1", "rustc"),
            ("1.99.1", "1.89.0", "cargo"),
        ):
            with self.subTest(refused=refused):
                tool(self.bin, "rustc", f"rustc {rustc} (00000000 2025-08-01)")
                tool(self.bin, "cargo", f"cargo {cargo} (00000000 2025-08-01)")
                self.assert_refused(
                    self.configure(),
                    f"yamaa needs {refused} {toolchain.minimum()} or later",
                    f"is {refused} 1.89.0",
                    "rustup update",
                )

    def test_the_minimum_is_the_declared_rust_version(self):
        tool(self.bin, "cargo", "cargo 1.95.0 (00000000 2026-01-01)")
        for declared, rustc, accepted in (
            ("1.95", "1.94.2", False),
            ("1.95", "1.95.0", True),
            ("1.90.1", "1.90.0", False),
            ("1.90", "1.90.0-nightly", True),
        ):
            with self.subTest(declared=declared, rustc=rustc):
                self.manifest(declared)
                tool(self.bin, "rustc", f"rustc {rustc} (00000000 2026-01-01)")
                result = self.configure()
                self.assertEqual(result.returncode == 0, accepted, result.stderr)
        # The repository's own declaration, which staging copies unchanged.
        shutil.copyfile(ROOT / "rust/Cargo.toml", self.package / "src/rust/Cargo.toml")
        result = self.configure("cat(minimum_rust())")
        self.assertEqual(result.stdout, toolchain.minimum(), result.stderr)
        configure = (PACKAGE / "tools/configure.R").read_text(encoding="utf-8")
        self.assertIsNone(re.search(r"\b1\.\d\d\b", configure))

    def test_a_failing_version_command_is_reported(self):
        tool(self.bin, "rustc", "error: no default toolchain is configured", status=1)
        tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        self.assert_refused(
            self.configure(),
            "'rustc --version' failed for",
            "error: no default toolchain is configured",
        )

    def test_windows_needs_the_gnu_target(self):
        sysroot = self.root / "sysroot"
        tool(
            self.bin,
            "rustc.exe",
            "rustc 1.99.1 (58cf8f9c3 2026-10-13)",
            sysroot=sysroot,
        )
        cargo = tool(self.bin, "cargo.exe", "cargo 1.99.1 (5f94df478 2026-08-27)")
        result = self.configure("main(windows = TRUE, darwin = FALSE)")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rustup target add x86_64-pc-windows-gnu", result.stderr)
        self.assertFalse((self.package / "src/Makevars.win").exists())
        library = sysroot / "lib/rustlib/x86_64-pc-windows-gnu/lib"
        library.mkdir(parents=True)
        (library / "libstd-0123456789abcdef.rlib").write_bytes(b"")
        result = self.configure("main(windows = TRUE, darwin = FALSE)")
        self.assertEqual(result.returncode, 0, result.stderr)
        makevars = self.makevars("Makevars.win")
        self.assertIn(f"\nCARGO = {cargo}\n", makevars)
        self.assertNotIn("@", makevars)
        self.assertFalse((self.package / "src/Makevars").exists())

    def test_macos_builds_for_the_deployment_target(self):
        tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        for environment, target in (
            ({}, "11.0"),
            ({"MACOSX_DEPLOYMENT_TARGET": "12.3"}, "12.3"),
        ):
            with self.subTest(target=target):
                result = self.configure(
                    "main(windows = FALSE, darwin = TRUE)", **environment
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(
                    f"\nCARGO_ENV = MACOSX_DEPLOYMENT_TARGET={target}\n",
                    self.makevars(),
                )


class BuildRulesTests(unittest.TestCase):
    def test_cargo_runs_with_at_most_two_jobs_and_never_installs_rust(self):
        # Cargo joins the jobserver of a parallel make, so -j 2 alone would not
        # bound it; rustup installs the toolchain that a rust-toolchain.toml in
        # a parent directory names unless RUSTUP_AUTO_INSTALL=0.
        for name in ("Makevars.in", "Makevars.win.in"):
            with self.subTest(name=name):
                rules = (PACKAGE / "src" / name).read_text(encoding="utf-8")
                commands = [
                    line
                    for line in rules.replace("\\\n", " ").splitlines()
                    if '"$(CARGO)"' in line
                ]
                self.assertEqual(len(commands), 1)
                command = commands[0]
                for setting in (
                    "MAKEFLAGS= ",
                    "MFLAGS= ",
                    "CARGO_MAKEFLAGS= ",
                    "RUSTUP_AUTO_INSTALL=0 ",
                    'RUSTC="$(RUSTC)"',
                    " build --locked --offline -j 2 ",
                ):
                    self.assertIn(setting, command)
                self.assertNotIn("$(HOME)", rules)

    def test_configure_scripts_run_the_shared_helper(self):
        for name, rscript in (
            ("configure", '"${R_HOME}/bin/Rscript"'),
            ("configure.win", '"${R_HOME}/bin${R_ARCH_BIN}/Rscript.exe"'),
        ):
            with self.subTest(name=name):
                path = PACKAGE / name
                self.assertTrue(os.access(path, os.X_OK))
                text = path.read_text(encoding="utf-8")
                self.assertTrue(text.startswith("#!/bin/sh\n"))
                self.assertIn(f"exec {rscript} --vanilla tools/configure.R\n", text)
        for name in ("cleanup", "cleanup.win"):
            with self.subTest(name=name):
                self.assertTrue(os.access(PACKAGE / name, os.X_OK))


if __name__ == "__main__":
    unittest.main()
