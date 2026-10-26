"""The R package's configure step: find, check and report Cargo and rustc.

These tests run R/yamaa/tools/configure.R, which configure and configure.win
run, against fake cargo and rustc commands, and run the engine recipe of the
Makevars that it writes with make. Configure tests need R; `make test-r` runs them.
Two tests run the recipes with the real Cargo of the Rust toolchain that the
workspace selects; set RUSTUP_TOOLCHAIN to run them with another, such as the
workspace rust-version.
"""

import os
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path

from tools.ci import toolchain
from tools.packaging import generate_component_inventory as inventory
from tools.packaging.check_cargo_lock import check_r_link
from tools.paths import ROOT, WORKSPACE
from tools.staging import vendor_crates

PACKAGE = ROOT / "R/yamaa"
RSCRIPT = shutil.which("Rscript")
MAKE = shutil.which("make")
SYSTEM_PATH = "/usr/bin:/bin"
SH = shutil.which("sh", path=SYSTEM_PATH)
# The component inventory that the source package carries and R installs.
INVENTORY = "inst/COMPONENTS.tsv"


def rust_toolchain():
    """The cargo and rustc of the Rust toolchain that the workspace selects,
    rather than rustup's proxies, and its host target; or None without one.

    Under rustup this is the pinned toolchain unless RUSTUP_TOOLCHAIN names
    another.
    """
    proxy = shutil.which("rustc")
    if proxy is None:
        return None

    def output(*arguments):
        return subprocess.run(
            [proxy, *arguments],
            cwd=WORKSPACE,
            env=os.environ | {"RUSTUP_AUTO_INSTALL": "0"},
            capture_output=True,
            text=True,
            check=True,
        ).stdout

    try:
        sysroot = Path(output("--print", "sysroot").strip())
        host = re.search(r"^host: (\S+)$", output("-vV"), re.MULTILINE)
    except (OSError, subprocess.CalledProcessError):
        return None
    cargo, rustc = sysroot / "bin/cargo", sysroot / "bin/rustc"
    if host is None or not (cargo.is_file() and rustc.is_file()):
        return None
    return cargo, rustc, host.group(1)


def tool(directory, name, line, status=0, sysroot=None, record=os.devnull):
    """Write a fake command that prints `line` for --version.

    Like a rustup proxy that may install a toolchain, it fails unless
    RUSTUP_AUTO_INSTALL=0. `--print sysroot` prints `sysroot`, and `build`
    writes the directory it runs in, CARGO_TARGET_DIR and CARGO_HOME to
    `record`.
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
              build) printf '%s\\n' "$(pwd -P)" "$CARGO_TARGET_DIR" "$CARGO_HOME" > "{record}" ;;
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
        for relative in (
            "tools/configure.R",
            "src/Makevars.in",
            "src/Makevars.win.in",
            INVENTORY,
        ):
            (self.package / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(PACKAGE / relative, self.package / relative)
        self.manifest(toolchain.minimum())
        self.bin = self.root / "bin"
        self.home = self.root / "home"
        self.home.mkdir()
        self.record = self.root / "cargo-build.txt"
        # A goal that makes only the engine library.
        self.goal = self.root / "engine.mk"
        self.goal.write_text("engine: $(RUST_LIB)\n", encoding="utf-8")

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

    def inventory_text(self):
        """The package's inventory, as R would install it."""
        return (self.package / INVENTORY).read_bytes().decode("utf-8")

    def files(self):
        """The bytes of every file below the temporary root, by path."""
        return {
            path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()
        }

    def assert_refused(self, result, *phrases):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        for phrase in phrases:
            self.assertIn(phrase, result.stderr)
        self.assertFalse((self.package / "src/Makevars").exists())

    def configured(self, windows, **environment):
        """Configure the package for Unix or Windows with fake tools whose
        cargo records what `build` is given, beside a one-crate bundle.
        Returns the Makevars name."""
        suffix = ".exe" if windows else ""
        sysroot = self.root / "sysroot"
        library = sysroot / "lib/rustlib/x86_64-pc-windows-gnu/lib"
        library.mkdir(parents=True, exist_ok=True)
        (library / "libstd-0123456789abcdef.rlib").write_bytes(b"")
        line = "1.99.1 (58cf8f9c3 2026-10-13)"
        tool(self.bin, "rustc" + suffix, f"rustc {line}", sysroot=sysroot)
        tool(self.bin, "cargo" + suffix, f"cargo {line}", record=self.record)
        crate = self.root / "crates/vendor/crate-1.0.0"
        crate.mkdir(parents=True, exist_ok=True)
        (crate / "Cargo.toml").write_text(
            '[package]\nname = "crate"\n', encoding="utf-8"
        )
        rust = self.package / "src/rust"
        with tarfile.open(rust / "vendor.tar.xz", "w:xz") as archive:
            archive.add(crate.parent, arcname=vendor_crates.DIRECTORY)
        (rust / "vendor-config.toml").write_text(vendor_crates.CONFIG, encoding="utf-8")
        platform = "TRUE" if windows else "FALSE"
        result = self.configure(
            f"main(windows = {platform}, darwin = FALSE)", **environment
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return "Makevars.win" if windows else "Makevars"

    def make(self, makevars, *arguments, **environment):
        """Run make for the engine library with the Makevars that configure
        wrote, in src/ as R CMD INSTALL does, with `arguments` before the
        goal."""
        if MAKE is None:
            self.skipTest("make is not installed")
        home = subprocess.run(
            [RSCRIPT, "--vanilla", "-e", "cat(R.home())"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        return subprocess.run(
            [MAKE, "-f", makevars, "-f", str(self.goal), *arguments, "engine"],
            cwd=self.package / "src",
            env={
                "PATH": f"{self.bin}:{SYSTEM_PATH}",
                "HOME": str(self.home),
                "LANG": "C.UTF-8",
                "R_HOME": home,
                **environment,
            },
            capture_output=True,
            text=True,
        )

    def build(self, makevars, **environment):
        """Make the engine library with the Makevars that configure wrote.

        Returns the target directory and the Cargo home that cargo was given,
        resolved against the directory that it ran in.
        """
        self.record.unlink(missing_ok=True)
        result = self.make(makevars, **environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        directory, target, cargo_home = self.record.read_text(
            encoding="utf-8"
        ).splitlines()
        return Path(directory, target).resolve(), Path(directory, cargo_home).resolve()

    def cargo_command(self, makevars, *arguments):
        """The words of the engine recipe's cargo command, as make prints it
        without running anything, with `arguments` on make's command line."""
        result = self.make(makevars, "-n", *arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        # The shell joins a line that ends in a backslash to the next.
        lines = result.stdout.replace("\\\n", "").splitlines()
        (command,) = [line for line in lines if " build " in line]
        return shlex.split(command)

    def bundle(self):
        """Replace the bundle's workspace with one that Cargo can build: a
        yamaa-r crate with no dependencies whose static library stands for
        the engine."""
        rust = self.package / "src/rust"
        crate = rust / "yamaa-r"
        (crate / "src").mkdir(parents=True, exist_ok=True)
        (rust / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["yamaa-r"]\nresolver = "2"\n\n'
            f'[workspace.package]\nrust-version = "{toolchain.minimum()}"\n',
            encoding="utf-8",
        )
        (rust / "Cargo.lock").write_text(
            'version = 3\n\n[[package]]\nname = "yamaa-r"\nversion = "0.1.0"\n',
            encoding="utf-8",
        )
        (crate / "Cargo.toml").write_text(
            '[package]\nname = "yamaa-r"\nversion = "0.1.0"\nedition = "2021"\n\n'
            '[lib]\ncrate-type = ["staticlib"]\n',
            encoding="utf-8",
        )
        (crate / "src/lib.rs").write_text(
            "pub fn answer() -> i32 {\n    42\n}\n", encoding="utf-8"
        )

    def configured_for_cargo(self, windows, rust):
        """Configure the package for Unix or Windows beside a bundle that
        Cargo can build, for the cargo, rustc and host target that
        rust_toolchain() found as `rust`.

        The toolchain's own tools stand in for configure's fake ones.
        Makevars.win builds for the host, whose standard library the
        toolchain has, with GNU make as on Rtools. Returns the Makevars name,
        the variables for make's command line and the engine library that
        the recipe builds.
        """
        cargo, rustc, host = rust
        makevars = self.configured(windows)
        self.bundle()
        variables = [f"CARGO={cargo}", f"RUSTC={rustc}"]
        release = self.package / "src/rust/target"
        if windows:
            variables.append(f"RUST_TARGET={host}")
            release = release / host
        return makevars, variables, release / "release/libyamaa_r.a"

    def outside_package(self):
        """Every path below the temporary root that is not in the package."""
        return {
            path
            for path in self.root.rglob("*")
            if not path.is_relative_to(self.package)
        }

    def test_tools_on_the_path_are_reported_and_written_into_makevars(self):
        rustc = tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        cargo = tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        # This case checks the plain Unix environment; the macOS case below
        # checks its deployment target independently of the host running tests.
        result = self.configure("main(windows = FALSE, darwin = FALSE)")
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
                rustc = tool(
                    directory, "rustc", f"rustc {toolchain.minimum()} (0 2024-10-15)"
                )
                cargo = tool(
                    directory, "cargo", f"cargo {toolchain.minimum()} (0 2024-10-15)"
                )
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
        major, minor, _ = map(int, toolchain.minimum().split("."))
        older = f"{major}.{minor - 1}.0"
        for rustc, cargo, refused in (
            (older, "1.99.1", "rustc"),
            ("1.99.1", older, "cargo"),
        ):
            with self.subTest(refused=refused):
                tool(self.bin, "rustc", f"rustc {rustc} (00000000 2024-08-01)")
                tool(self.bin, "cargo", f"cargo {cargo} (00000000 2024-08-01)")
                self.assert_refused(
                    self.configure(),
                    f"yamaa needs {refused} {toolchain.minimum()} or later",
                    f"is {refused} {older}",
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

    def test_what_configure_writes_strips_nothing_at_the_link_step(self):
        # R's linker links the engine into the shared object with the Makevars
        # that configure writes, and CRAN's Repository Policy asks a package
        # not to strip it.
        sysroot = self.root / "sysroot"
        library = sysroot / "lib/rustlib/x86_64-pc-windows-gnu/lib"
        library.mkdir(parents=True)
        (library / "libstd-0123456789abcdef.rlib").write_bytes(b"")
        line = "1.99.1 (58cf8f9c3 2026-10-13)"
        for suffix in ("", ".exe"):
            tool(self.bin, "rustc" + suffix, f"rustc {line}", sysroot=sysroot)
            tool(self.bin, "cargo" + suffix, f"cargo {line}")
        for platform, name in (
            ("windows = FALSE, darwin = FALSE", "Makevars"),
            ("windows = FALSE, darwin = TRUE", "Makevars"),
            ("windows = TRUE, darwin = FALSE", "Makevars.win"),
        ):
            with self.subTest(platform=platform):
                result = self.configure(f"main({platform})")
                self.assertEqual(result.returncode, 0, result.stderr)
                check_r_link(f"src/{name}", self.makevars(name))

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

    def test_the_inventory_names_the_standard_library_that_rustc_reports(self):
        # R installs inst/COMPONENTS.tsv with the package, so configure names
        # there the standard library that rustc links into it (#3030 D117):
        # one rust-std row with the release that rustc reports, after the
        # source package's rows, which is what check_package_notices requires
        # of an R binary package built with the pinned toolchain.
        committed = (PACKAGE / INVENTORY).read_text(encoding="utf-8")
        self.assertNotIn("\nrust-std\t", committed)
        major, minor, _ = map(int, toolchain.minimum().split("."))
        tool(self.bin, "cargo", f"cargo {toolchain.channel()} (5f94df478 2026-08-27)")
        for line, release in (
            (
                f"rustc {toolchain.channel()} (58cf8f9c3 2026-10-13)",
                toolchain.channel(),
            ),
            (
                f"rustc {toolchain.minimum()} (4d91de4e4 2024-10-15) (built from a "
                "source tarball)",
                toolchain.minimum(),
            ),
        ):
            with self.subTest(release=release):
                shutil.copyfile(PACKAGE / INVENTORY, self.package / INVENTORY)
                tool(self.bin, "rustc", line)
                result = self.configure("main(windows = FALSE, darwin = FALSE)")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    self.inventory_text(),
                    inventory.installed_inventory(committed, release),
                )
        # A pre-release keeps its suffix: its standard library is not the
        # release's.
        nightly = f"{major}.{minor + 1}.0-nightly"
        tool(self.bin, "rustc", f"rustc {nightly} (00000000 2026-01-01)")
        result = self.configure("main(windows = FALSE, darwin = FALSE)")
        self.assertEqual(result.returncode, 0, result.stderr)
        *rows, row = self.inventory_text().splitlines(keepends=True)
        self.assertEqual("".join(rows), committed)
        self.assertEqual(
            row.split("\t"),
            [
                "rust-std",
                nightly,
                "MIT",
                f"https://github.com/rust-lang/rust at {nightly}",
                inventory.INSTALLED_PURPOSE + "\n",
            ],
        )

    def test_a_rerun_leaves_one_standard_library_row(self):
        # R CMD INSTALL configures a package directory each time that it
        # installs from it, with the same rustc or another. The inventory
        # keeps one row, for the rustc of the last run.
        committed = (PACKAGE / INVENTORY).read_text(encoding="utf-8")
        tool(self.bin, "cargo", f"cargo {toolchain.channel()} (5f94df478 2026-08-27)")
        for release in (toolchain.channel(), toolchain.channel(), toolchain.minimum()):
            tool(self.bin, "rustc", f"rustc {release} (58cf8f9c3 2026-10-13)")
            result = self.configure("main(windows = FALSE, darwin = FALSE)")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                self.inventory_text(),
                inventory.installed_inventory(committed, release),
            )
            self.assertEqual(self.inventory_text().count("\nrust-std\t"), 1)

    def test_configure_writes_only_its_makevars_and_the_inventory(self):
        # CRAN's policy admits no writing outside the build tree and the
        # temporary directory (#3119, #3030 D86). In the package, configure
        # writes the Makevars and the inventory that R installs, and nothing
        # else.
        sysroot = self.root / "sysroot"
        library = sysroot / "lib/rustlib/x86_64-pc-windows-gnu/lib"
        library.mkdir(parents=True)
        (library / "libstd-0123456789abcdef.rlib").write_bytes(b"")
        line = f"{toolchain.channel()} (58cf8f9c3 2026-10-13)"
        for suffix in ("", ".exe"):
            tool(self.bin, "rustc" + suffix, f"rustc {line}", sysroot=sysroot)
            tool(self.bin, "cargo" + suffix, f"cargo {line}")
        for platform, name in (
            ("windows = FALSE, darwin = FALSE", "Makevars"),
            ("windows = FALSE, darwin = TRUE", "Makevars"),
            ("windows = TRUE, darwin = FALSE", "Makevars.win"),
        ):
            with self.subTest(platform=platform):
                (self.package / "src" / name).unlink(missing_ok=True)
                shutil.copyfile(PACKAGE / INVENTORY, self.package / INVENTORY)
                before = self.files()
                result = self.configure(f"main({platform})")
                self.assertEqual(result.returncode, 0, result.stderr)
                after = self.files()
                self.assertEqual(
                    {
                        path
                        for path in before.keys() | after.keys()
                        if before.get(path) != after.get(path)
                    },
                    {self.package / "src" / name, self.package / INVENTORY},
                )

    def test_a_package_without_its_inventory_is_refused(self):
        tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        (self.package / INVENTORY).unlink()
        self.assert_refused(
            self.configure(),
            "inst/COMPONENTS.tsv is missing; the package must carry its component "
            "inventory.",
        )

    @unittest.skipIf(SH is None, "no POSIX shell")
    def test_cleanup_restores_the_inventory_that_the_source_package_carries(self):
        # R CMD build runs cleanup, so a package directory that has been
        # configured still builds a source archive with the committed
        # inventory, which check_package_notices requires. "Writing R
        # Extensions" asks for a cleanup that removes what configure creates.
        committed = (PACKAGE / INVENTORY).read_bytes()
        tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        for name in ("cleanup", "cleanup.win"):
            shutil.copy2(PACKAGE / name, self.package / name)
            with self.subTest(script=name):
                result = self.configure("main(windows = FALSE, darwin = FALSE)")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotEqual((self.package / INVENTORY).read_bytes(), committed)
                # Again on the cleaned directory, where it changes nothing.
                for _ in range(2):
                    cleaned = subprocess.run(
                        [SH, name],
                        cwd=self.package,
                        env={"PATH": SYSTEM_PATH, "LANG": "C"},
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(cleaned.returncode, 0, cleaned.stderr)
                    self.assertEqual((self.package / INVENTORY).read_bytes(), committed)
                    self.assertEqual(
                        [path.name for path in (self.package / "inst").iterdir()],
                        ["COMPONENTS.tsv"],
                    )
                    self.assertFalse((self.package / "src/Makevars").exists())

    def test_the_build_stays_in_the_package_whatever_cargo_target_dir_says(self):
        # CRAN's policy admits no writing outside the build tree and the
        # temporary directory, so a CARGO_TARGET_DIR that the caller sets for
        # other projects must not move the build.
        outside = self.root / "elsewhere"
        own = self.package / "src/rust/target"
        for windows, unset in (
            (False, {}),
            (True, {}),
            (False, {"YAMAA_R_CARGO_TARGET_DIR": ""}),
        ):
            environment = {"CARGO_TARGET_DIR": str(outside), **unset}
            with self.subTest(windows=windows, environment=environment):
                makevars = self.configured(windows, **environment)
                target, home = self.build(makevars, **environment)
                self.assertEqual(target, own.resolve())
                self.assertEqual(home, own.resolve() / "cargo-home")
                self.assertTrue((own / "vendor/crate-1.0.0/Cargo.toml").is_file())
                self.assertTrue((own / "cargo-home/config.toml").is_file())
                self.assertFalse(outside.exists())
                shutil.rmtree(own)

    def test_yamaa_r_cargo_target_dir_moves_the_whole_build(self):
        # Installations that name one directory share Cargo's build there, as
        # CI's do. Cargo's home and the unpacked crates move with it: their
        # paths enter Cargo's fingerprints, and the package's own directory
        # differs between installations.
        shared = self.root / "shared/target"
        environment = {
            "YAMAA_R_CARGO_TARGET_DIR": str(shared),
            "CARGO_TARGET_DIR": str(self.root / "elsewhere"),
        }
        for windows in (False, True):
            with self.subTest(windows=windows):
                makevars = self.configured(windows, **environment)
                target, home = self.build(makevars, **environment)
                self.assertEqual(target, shared.resolve())
                self.assertEqual(home, shared.resolve() / "cargo-home")
                self.assertTrue((shared / "vendor/crate-1.0.0/Cargo.toml").is_file())
                self.assertTrue((shared / "cargo-home/config.toml").is_file())
                self.assertFalse((self.package / "src/rust/target").exists())
                self.assertFalse((self.root / "elsewhere").exists())
                shutil.rmtree(shared)
        result = self.configure("main(windows = FALSE, darwin = FALSE)", **environment)
        self.assertEqual(
            result.stdout.splitlines()[-1],
            f"Cargo builds in {shared}, which YAMAA_R_CARGO_TARGET_DIR names",
        )

    def test_yamaa_r_cargo_target_dir_leaves_out_only_the_isolation(self):
        # Unset, the cargo command carries the settings that keep the build in
        # the package. Set, as in CI, the build leaves the package anyway, and
        # the command differs only by its directory and by lacking them, so
        # inherited Cargo settings apply as they do to any Cargo build.
        shared = self.root / "shared/target"
        for windows in (False, True):
            with self.subTest(windows=windows):
                # make expands CURDIR to the directory where it runs.
                own = (
                    f"{(self.package / 'src').resolve()}/rust/target"
                    if windows
                    else "rust/target"
                )
                # The build directory is relative on Windows too, so that no
                # path of the build tree enters Cargo's template.
                settings = [
                    "RUSTC_WRAPPER=",
                    "RUSTC_WORKSPACE_WRAPPER=",
                    "CARGO_BUILD_BUILD_DIR=rust/target",
                ]
                options = [
                    "--config",
                    "build.rustc-wrapper=''",
                    "--config",
                    "build.rustc-workspace-wrapper=''",
                ]
                isolated = self.cargo_command(self.configured(windows))
                cargo = next(
                    index
                    for index, word in enumerate(isolated)
                    if word.endswith(("/cargo", "/cargo.exe"))
                )
                self.assertLessEqual(set(settings), set(isolated[:cargo]))
                self.assertEqual(isolated[-len(options) :], options)
                makevars = self.configured(
                    windows, YAMAA_R_CARGO_TARGET_DIR=str(shared)
                )
                self.assertEqual(
                    [
                        word.replace(str(shared), own)
                        for word in self.cargo_command(makevars)
                    ],
                    [word for word in isolated if word not in settings + options],
                )

    def test_the_c_that_crates_compile_uses_the_c_compiler_of_r(self):
        # The cc crate compiles the ReadStat and zstd sources. A sanitizer
        # build of R, such as R-hub's clang-asan image, adds its flags to the
        # CC of R's Makeconf, given here on make's command line, so each
        # recipe hands that whole command to Cargo for the target it builds.
        # R's CFLAGS, set for a package's own sources, stay out.
        compiler = (
            "clang-23 -std=gnu23 -fsanitize=address,undefined -fno-omit-frame-pointer"
        )
        for windows, variable in ((False, "CC"), (True, "CC_x86_64_pc_windows_gnu")):
            with self.subTest(windows=windows):
                command = self.cargo_command(
                    self.configured(windows),
                    f"CC={compiler}",
                    "CFLAGS=-O3 -Wall -pedantic",
                )
                cargo = next(
                    index
                    for index, word in enumerate(command)
                    if word.endswith(("/cargo", "/cargo.exe"))
                )
                self.assertEqual(
                    [
                        word
                        for word in command[:cargo]
                        if word.startswith(("CC", "CFLAGS"))
                    ],
                    [f"{variable}={compiler}"],
                )

    def test_the_windows_linker_is_the_program_of_the_c_compiler_of_r(self):
        # Cargo runs its linker setting as one program, and on a GNU host, as
        # on CRAN's Windows machines, links the build scripts with it. R's
        # Makeconf there ends CC in a space, and Cargo found no linker
        # "gcc " (#4005). Flags in CC stay with the cc crate.
        makevars = self.configured(True)
        for compiler in ("gcc ", "gcc", "gcc -std=gnu2x"):
            with self.subTest(compiler=compiler):
                command = self.cargo_command(makevars, f"CC={compiler}")
                self.assertEqual(
                    [
                        word
                        for word in command
                        if word.startswith("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=")
                    ],
                    ["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=gcc"],
                    compiler,
                )

    def test_no_inherited_build_directory_or_rustc_wrapper_reaches_cargo(self):
        # CRAN's policy admits no writing outside the build tree and the
        # temporary directory. So unless YAMAA_R_CARGO_TARGET_DIR is set, a
        # build directory or a rustc wrapper that the caller's environment or
        # a .cargo/config.toml in a parent directory names must neither move
        # Cargo's output nor run (#3030 D86). Both recipes run here with a
        # real Cargo on a one-crate bundle.
        rust = rust_toolchain()
        if rust is None:
            self.skipTest("no Rust toolchain")
        runs = self.root / "wrapper-runs"
        wrapper = self.root / "wrapper"
        wrapper.write_text(
            f'#!/bin/sh\necho "$*" >> "{runs}"\nexec "$@"\n', encoding="utf-8"
        )
        wrapper.chmod(0o755)
        outside = self.root / "build-dir"
        parent = self.root / ".cargo/config.toml"
        parent.parent.mkdir()
        # Each setting in the environment, or in the parent's configuration.
        settings = [
            ({"CARGO_BUILD_BUILD_DIR": str(outside)}, ""),
            *(
                ({name: str(wrapper)}, "")
                for name in (
                    "RUSTC_WRAPPER",
                    "RUSTC_WORKSPACE_WRAPPER",
                    "CARGO_BUILD_RUSTC_WRAPPER",
                    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
                )
            ),
            ({}, f'build-dir = "{outside}"'),
            ({}, f'rustc-wrapper = "{wrapper}"'),
            ({}, f'rustc-workspace-wrapper = "{wrapper}"'),
        ]
        own = self.package / "src/rust/target"

        def run_recipe(makevars, environment, configuration, *arguments):
            shutil.rmtree(own, ignore_errors=True)
            shutil.rmtree(outside, ignore_errors=True)
            runs.unlink(missing_ok=True)
            parent.write_text(f"[build]\n{configuration}\n", encoding="utf-8")
            before = self.outside_package()
            result = self.make(makevars, *arguments, **environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            return self.outside_package() - before

        for windows in (False, True):
            makevars, variables, library = self.configured_for_cargo(windows, rust)
            # Without the isolation, as with YAMAA_R_CARGO_TARGET_DIR set, the
            # caller's wrapper runs, and the check below would see it.
            for environment, configuration in (
                ({"RUSTC_WRAPPER": str(wrapper)}, ""),
                ({}, f'rustc-wrapper = "{wrapper}"'),
            ):
                with self.subTest(
                    windows=windows, isolation=False, configuration=configuration
                ):
                    arguments = [*variables, "ISOLATION_ENV=", "ISOLATION_CONFIG="]
                    written = run_recipe(
                        makevars, environment, configuration, *arguments
                    )
                    self.assertIn(runs, written)
            for environment, configuration in settings:
                with self.subTest(
                    windows=windows,
                    environment=environment,
                    configuration=configuration,
                ):
                    written = run_recipe(
                        makevars, environment, configuration, *variables
                    )
                    self.assertTrue(library.is_file())
                    self.assertEqual(written, set())

    def test_braces_in_the_build_path_neither_stop_nor_move_the_build(self):
        # R's temporary directory, or the directory that a user installs from,
        # may hold a { or }. Cargo 1.91 and later read the build directory as
        # a template, so a build directory holding such a path would stop
        # Cargo ("unexpected variable", "unexpected opening bracket"), or, with
        # braces around one of Cargo's template variables, send its output out
        # of the package (#3275). Both recipes run here with a real Cargo.
        rust = rust_toolchain()
        if rust is None:
            self.skipTest("no Rust toolchain")
        for name in ("path{x}", "{", "}", "{workspace-path-hash}"):
            package = self.root / name / "yamaa"
            package.parent.mkdir()
            self.package = self.package.rename(package)
            for windows in (False, True):
                with self.subTest(directory=name, windows=windows):
                    makevars, variables, library = self.configured_for_cargo(
                        windows, rust
                    )
                    shutil.rmtree(self.package / "src/rust/target", ignore_errors=True)
                    before = self.outside_package()
                    result = self.make(makevars, *variables)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue(library.is_file())
                    self.assertEqual(self.outside_package() - before, set())

    def test_a_target_directory_that_make_cannot_read_is_refused(self):
        tool(self.bin, "rustc", "rustc 1.99.1 (58cf8f9c3 2026-10-13)")
        tool(self.bin, "cargo", "cargo 1.99.1 (5f94df478 2026-08-27)")
        # Relative to the package, which is a temporary copy, or holding what
        # make or the shell would read otherwise.
        for directory in (
            "rust/target",
            "C:target",
            "/srv/a b",
            "/srv/it's",
            "/srv/$HOME",
        ):
            with self.subTest(directory=directory):
                self.assert_refused(
                    self.configure(YAMAA_R_CARGO_TARGET_DIR=directory),
                    f"YAMAA_R_CARGO_TARGET_DIR is '{directory}'.",
                    "It must be an absolute path",
                )
        for directory, written in (
            ("/srv/ci/rust/target", "/srv/ci/rust/target"),
            ("D:\\a\\yamaa\\yamaa\\rust\\target", "D:/a/yamaa/yamaa/rust/target"),
        ):
            with self.subTest(directory=directory):
                result = self.configure(
                    "main(windows = FALSE, darwin = FALSE)",
                    YAMAA_R_CARGO_TARGET_DIR=directory,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"\nCARGO_TARGET_DIR = {written}\n", self.makevars())


class BuildRulesTests(unittest.TestCase):
    def test_cargo_runs_with_at_most_two_jobs_and_never_installs_rust(self):
        # Cargo joins the jobserver of a parallel make, so -j 2 alone would not
        # bound it; rustup installs the toolchain that a rust-toolchain.toml in
        # a parent directory names unless RUSTUP_AUTO_INSTALL=0. The command
        # also names the target directory, which a CARGO_TARGET_DIR in the
        # caller's environment would otherwise set.
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
                    'CARGO_TARGET_DIR="$(CARGO_TARGET_DIR)"',
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


@unittest.skipIf(MAKE is None or SH is None, "POSIX make and sh are not installed")
class RustExportsTests(unittest.TestCase):
    """The Windows DLL link prepares the archive outside R's nested build path."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.temporary = self.root / "temporary files"
        self.temporary.mkdir()
        self.archive = self.root / ("nested-" * 20) / ("install-" * 10) / "libyamaa_r.a"
        self.archive.parent.mkdir(parents=True)
        self.archive.write_bytes(b"original archive")
        self.dll = self.root / "yamaa.dll"
        self.record = self.root / "objcopy-directory.txt"
        self.objcopy = self.root / "objcopy"
        # GNU objcopy extracts archive members beside its input. Model the
        # Windows limit with the long member name that a Rust archive can hold.
        member = "yamaa_r-" + "rustc_std_workspace_alloc-" * 5 + "cgu.0.rcgu.o"
        self.assertGreater(len(str(self.archive.parent / "sta00000" / member)), 260)
        self.objcopy.write_text(
            f"#!{sys.executable}\n"
            "import os, sys\nfrom pathlib import Path\n"
            "assert sys.argv[1] == '--remove-section=.drectve'\n"
            "archive = Path(sys.argv[2])\n"
            f"Path({str(self.record)!r}).write_text(str(archive.parent))\n"
            f"if len(str(archive.parent / 'sta00000' / {member!r})) >= 260:\n"
            "    raise SystemExit('archive member path exceeds MAX_PATH')\n"
            "assert archive.read_bytes() == b'original archive'\n"
            "archive.write_bytes(b'without directives')\n"
            "if os.environ.get('OBJCOPY_FAIL') == '1':\n"
            "    raise SystemExit('simulated objcopy failure')\n",
            encoding="utf-8",
        )
        self.objcopy.chmod(0o755)
        self.rules = self.root / "link.mk"
        self.rules.write_text(
            "$(RUST_LIB):\n\t@:\n$(SHLIB):\n"
            '\ttest "$$(cat "$(RUST_LIB)")" = "without directives" && touch "$@"\n',
            encoding="utf-8",
        )

    def link(self, failure=False):
        if MAKE is None:
            self.skipTest("make is not installed")
        return subprocess.run(
            [
                MAKE,
                "-f",
                str(PACKAGE / "src/Makevars.win.in"),
                "-f",
                str(self.rules),
                f"RUST_LIB={self.archive}",
                f"SHLIB={self.dll}",
                f"OBJCOPY={self.objcopy}",
                str(self.dll),
            ],
            cwd=self.root,
            env=os.environ
            | {"TMPDIR": str(self.temporary), "OBJCOPY_FAIL": "1" if failure else "0"},
            capture_output=True,
            text=True,
        )

    def test_long_archive_member_names_are_prepared_in_a_temporary_directory(self):
        result = self.link()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.archive.read_bytes(), b"without directives")
        self.assertTrue(self.dll.is_file())
        self.assertEqual(Path(self.record.read_text()).parent, self.temporary)
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_a_failed_archive_edit_leaves_the_original_and_links_nothing(self):
        result = self.link(failure=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("simulated objcopy failure", result.stderr)
        self.assertEqual(self.archive.read_bytes(), b"original archive")
        self.assertFalse(self.dll.exists())
        self.assertEqual(list(self.temporary.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
