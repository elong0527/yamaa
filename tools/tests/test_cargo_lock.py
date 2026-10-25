"""Exercise real Cargo refusal and the engine bundles that both source archives carry."""

import io
import json
import re
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_cargo_lock as guard
from tools.staging import engine_bundle


def archive(path, entries, mode="w:gz"):
    """Write (name, payload) entries; a str payload is a link's target."""
    with tarfile.open(path, mode) as output:
        for name, value in entries:
            member = tarfile.TarInfo(name)
            if isinstance(value, bytes):
                member.size = len(value)
                output.addfile(member, io.BytesIO(value))
            else:
                member.type = tarfile.SYMTYPE
                member.linkname = value
                output.addfile(member)


def packed(entries):
    """The bytes of an xz tar of (name, payload) entries, as archive() writes."""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "vendor.tar.xz"
        archive(path, entries, "w:xz")
        return path.read_bytes()


class CargoLockTests(unittest.TestCase):
    def test_missing_and_stale_locks_refuse_without_changing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            (workspace / "src").mkdir()
            (workspace / "src/lib.rs").write_text("pub fn unchanged() {}\n")
            manifest = workspace / "Cargo.toml"
            manifest.write_text(
                '[package]\nname = "lock-witness"\nversion = "0.1.0"\nedition = "2021"\n'
            )
            with self.assertRaisesRegex(ValueError, "missing"):
                guard.locked_metadata(workspace)
            self.assertFalse((workspace / "Cargo.lock").exists())
            subprocess.run(
                ["cargo", "generate-lockfile", "--offline"],
                cwd=workspace,
                check=True,
                capture_output=True,
            )
            lock = (workspace / "Cargo.lock").read_bytes()
            guard.locked_metadata(workspace)
            manifest.write_text(manifest.read_text().replace('"0.1.0"', '"0.2.0"'))
            with self.assertRaises(subprocess.CalledProcessError):
                guard.locked_metadata(workspace)
            self.assertEqual((workspace / "Cargo.lock").read_bytes(), lock)


REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def lock_text(packages):
    lines = ["version = 4"]
    for name, version, registry, dependencies in packages:
        lines += ["", "[[package]]", f'name = "{name}"', f'version = "{version}"']
        if registry:
            lines += [f'source = "{REGISTRY}"', f'checksum = "recorded {name}"']
        if dependencies:
            lines.append(
                "dependencies = [" + ", ".join(f'"{d}"' for d in dependencies) + "]"
            )
    return "\n".join(lines) + "\n"


class BundleArchiveTests(unittest.TestCase):
    """Each host's source archive bundle, against a small checkout."""

    ENGINE = [
        (
            "yamaa",
            "0.5.0",
            False,
            ["yamaa-adapters", "yamaa-application", "yamaa-core"],
        ),
        ("yamaa-adapters", "0.5.0", False, ["parser", "yamaa-core"]),
        ("yamaa-application", "0.5.0", False, ["yamaa-adapters", "yamaa-core"]),
        ("yamaa-core", "0.5.0", False, []),
        ("parser", "1.0.0", True, []),
    ]
    # The boundary crate, which both bindings depend on.
    BOUNDARY = [("yamaa-boundary", "0.5.0", False, [])]
    BINDINGS = {
        "python": [
            ("yamaa-python", "0.5.0", False, ["pyo3", "yamaa", "yamaa-boundary"]),
            ("pyo3", "0.27.2", True, []),
        ],
        "r": [
            ("extendr", "0.9.0", True, []),
            ("yamaa-r", "0.5.0", False, ["extendr", "yamaa", "yamaa-boundary"]),
        ],
    }
    # Each host's vendored crates, by directory, with their files.
    CRATES = {
        "python": {
            "parser-1.0.0": {"Cargo.toml": b""},
            "pyo3-0.27.2": {"Cargo.toml": b"", "src/lib.rs": b"//\n"},
        },
        "r": {
            "parser-1.0.0": {"Cargo.toml": b""},
            "extendr-0.9.0": {"Cargo.toml": b"", "src/lib.rs": b"//\n"},
        },
    }

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.workspace = self.root / "rust"
        crates = (
            "yamaa-core",
            "yamaa-adapters",
            "yamaa-application",
            "yamaa",
            "yamaa-boundary",
            "yamaa-python",
            "yamaa-r",
        )
        for crate in crates:
            (self.workspace / "crates" / crate / "src").mkdir(parents=True)
            (self.workspace / "crates" / crate / "Cargo.toml").write_text(
                f'[package]\nname = "{crate}"\n'
            )
            (self.workspace / "crates" / crate / "src/lib.rs").write_text("//\n")
        listed = ", ".join(f'"crates/{crate}"' for crate in crates)
        # The wheels' stripping profile, which the R bundle leaves out.
        (self.workspace / "Cargo.toml").write_text(
            f'[workspace]\nmembers = [{listed}]\nresolver = "2"\n' + DIST
        )
        self.canonical = lock_text(
            self.ENGINE + self.BOUNDARY + self.BINDINGS["python"] + self.BINDINGS["r"]
        )
        (self.workspace / "Cargo.lock").write_text(self.canonical)
        # One selftest case, whose files each bundle stages in the facade.
        case = self.root / "benchmarks/case-one"
        (case / "expected").mkdir(parents=True)
        (case / "spec.yaml").write_text("domain: TEST\n")
        (case / "expected/out.csv").write_text("A\n1\n")
        (case / "README.md").write_text("prose\n")
        (self.root / "benchmarks/selftest.yaml").write_text("cases: [case-one]\n")
        # The schema copies that the adapters embed, which each bundle carries.
        schema = self.workspace / "crates/yamaa-adapters/schema"
        (schema / "json").mkdir(parents=True)
        (schema / "schema_spec.yaml").write_text('version: "1.0"\n')
        (schema / "json/spec.schema.json").write_text("{}\n")
        # And the closed Parquet profile that the adapters' reader embeds.
        profile = self.workspace / "crates/yamaa-adapters/profile"
        profile.mkdir()
        (profile / "parquet.toml").write_text("[levels]\n")
        self.path = self.root / "source.tar.gz"

    def vendored(self, crates, unlisted=()):
        """The vendor/ files of crates, each with a checksum record that lists
        its files but the `unlisted` ones."""
        files = {}
        for crate, contents in crates.items():
            listed = [name for name in contents if f"{crate}/{name}" not in unlisted]
            record = {"files": dict.fromkeys(listed, "recorded"), "package": "p"}
            files[f"vendor/{crate}/.cargo-checksum.json"] = json.dumps(record).encode()
            for name, payload in contents.items():
                files[f"vendor/{crate}/{name}"] = payload
        return files

    def entries(self, host, vendored=None, links=()):
        """A host's source archive files: its bundle and a package file.

        `links` are (name, target) links among the vendored crates."""
        layout = guard.LAYOUTS[host]
        entries = dict(guard.bundle_files(host, self.workspace))
        entries[layout.workspace + "Cargo.lock"] = lock_text(
            self.ENGINE + self.BOUNDARY + self.BINDINGS[host]
        ).encode()
        if vendored is None:
            vendored = self.vendored(self.CRATES[host])
        if layout.packed:
            entries[layout.packed] = packed([*vendored.items(), *links])
            entries["DESCRIPTION"] = b"Package: yamaa\n"
            for name in guard.R_MAKEVARS:
                entries[name] = (guard.R_PACKAGE / name).read_bytes()
        else:
            self.assertEqual(links, ())
            entries.update(vendored)
            entries["pyproject.toml"] = b"[project]\n"
        return entries

    def check(self, entries, links=()):
        archive(
            self.path,
            [(f"yamaa/{name}", payload) for name, payload in entries.items()]
            + [(f"yamaa/{name}", target) for name, target in links],
        )
        return guard.check_bundle_archive(self.path, self.workspace)

    def test_each_host_bundle_from_the_checkout_is_admitted(self):
        for host in ("python", "r"):
            with self.subTest(host=host):
                entries = self.entries(host)
                found, kept = self.check(entries)
                self.assertEqual(found, host)
                self.assertEqual(
                    sorted((name, version) for name, version, _ in kept),
                    sorted(
                        (name, version)
                        for name, version, _, _ in self.ENGINE
                        + self.BOUNDARY
                        + self.BINDINGS[host]
                    ),
                )
                layout = guard.LAYOUTS[host]
                manifest = entries[layout.workspace + "Cargo.toml"]
                self.assertEqual(
                    manifest,
                    engine_bundle.workspace_manifest(host, self.workspace).encode(),
                )
                self.assertEqual(b"[profile.dist]" in manifest, host == "python")
                self.assertEqual(
                    entries[layout.config], engine_bundle.CARGO_CONFIG.encode()
                )
                # Every archive carries the selftest cases in the facade crate,
                # with the checkout's bytes, and not their prose.
                staged = layout.workspace + "crates/yamaa/selftest/case-one/"
                self.assertEqual(
                    sorted(name for name in entries if name.startswith(staged)),
                    [staged + "expected/out.csv", staged + "spec.yaml"],
                )
                self.assertEqual(entries[staged + "expected/out.csv"], b"A\n1\n")
                # And the schema copies in the adapters crate, with their bytes.
                copies = layout.workspace + "crates/yamaa-adapters/schema/"
                self.assertEqual(
                    {
                        name: payload
                        for name, payload in entries.items()
                        if name.startswith(copies)
                    },
                    {
                        copies + "json/spec.schema.json": b"{}\n",
                        copies + "schema_spec.yaml": b'version: "1.0"\n',
                    },
                )
                profile = (
                    layout.workspace + "crates/yamaa-adapters/profile/parquet.toml"
                )
                self.assertEqual(entries[profile], b"[levels]\n")

    def test_only_a_manifest_alone_may_have_an_empty_unrecorded_library(self):
        # tools/staging/vendor_crates.py reduces a crate that no supported
        # target compiles to its manifest, with an empty src/lib.rs, which its
        # checksum record does not list, where Cargo needs one to read it.
        library = "parser-1.0.0/src/lib.rs"
        for host in ("python", "r"):
            for contents, admitted in (
                ({"src/lib.rs": b""}, True),
                ({"src/lib.rs": b"//\n"}, False),
                ({"src/lib.rs": b"", "src/main.rs": b"//\n"}, False),
            ):
                parser = {**self.CRATES[host]["parser-1.0.0"], **contents}
                crates = dict(self.CRATES[host], **{"parser-1.0.0": parser})
                entries = self.entries(host, self.vendored(crates, (library,)))
                with self.subTest(host=host, contents=contents):
                    if admitted:
                        self.assertEqual(self.check(entries)[0], host)
                    else:
                        with self.assertRaisesRegex(
                            ValueError, "parser-1.0.0 differs from the files its"
                        ):
                            self.check(entries)

    def test_no_toolchain_file_anywhere(self):
        for host, name in (
            ("python", "rust-toolchain.toml"),
            ("python", "rust/rust-toolchain.toml"),
            ("python", "vendor/pyo3-0.27.2/rust-toolchain"),
            ("python", "src/yamaa/rust-toolchain.toml"),
            ("r", "src/rust/rust-toolchain.toml"),
            ("r", "src/rust/crates/yamaa-r/rust-toolchain"),
            ("r", "inst/rust-toolchain.toml"),
        ):
            with self.subTest(host=host, name=name):
                with self.assertRaisesRegex(ValueError, f"carries {name}$"):
                    self.check(dict(self.entries(host), **{name: b""}))
        # In the R source package, nor inside its compressed crates, listed
        # in their checksum records or not.
        for crate, unlisted in (
            ({"rust-toolchain.toml": b"[toolchain]\n"}, ()),
            (
                {"tests/rust-toolchain": b"nightly\n"},
                ("extendr-0.9.0/tests/rust-toolchain",),
            ),
        ):
            crates = {
                **self.CRATES["r"],
                "extendr-0.9.0": {**self.CRATES["r"]["extendr-0.9.0"], **crate},
            }
            name = f"vendor/extendr-0.9.0/{next(iter(crate))}"
            with self.subTest(name=name):
                with self.assertRaisesRegex(
                    ValueError, f"src/rust/vendor.tar.xz carries {name}$"
                ):
                    self.check(self.entries("r", self.vendored(crates, unlisted)))

    def test_python_files_beside_or_changed_from_the_bundle_refuse(self):
        entries = self.entries("python")
        for name, payload, message in (
            (
                "rust/crates/yamaa-core/tests/case.rs",
                b"//\n",
                "checkout at rust/crates",
            ),
            ("rust/crates/yamaa/public-api.txt", b"api\n", "checkout at rust/crates"),
            (
                "rust/crates/yamaa/selftest/case-one/README.md",
                b"prose\n",
                "checkout at rust/crates/yamaa/selftest",
            ),
            ("rust/crates/yamaa-core/src/lib.rs", b"changed\n", "differs from the"),
            (
                "rust/crates/yamaa/selftest/case-one/expected/out.csv",
                b"A\n2\n",
                "selftest/case-one/expected/out.csv differs from the checkout",
            ),
            (
                "rust/crates/yamaa-adapters/schema/schema_spec.yaml",
                b'version: "2.0"\n',
                "schema/schema_spec.yaml differs from the checkout",
            ),
            (
                "rust/crates/yamaa-adapters/schema/schema_retired.yaml",
                b'version: "1.0"\n',
                "checkout at rust/crates/yamaa-adapters/schema/schema_retired",
            ),
            (".cargo/config.toml", b"[net]\noffline = false\n", "differs from the"),
            (
                "rust/Cargo.lock",
                self.canonical.encode(),
                "extendr 0.9.0 is outside the dependency",
            ),
            ("vendor/pyo3-0.27.2/src/extra.rs", b"", "files its .cargo-checksum"),
            ("vendor/extendr-0.9.0/Cargo.toml", b"", "vendored crate extendr-0.9.0"),
        ):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                self.check(dict(entries, **{name: payload}))
        for missing, message in (
            ("rust/crates/yamaa-python/src/lib.rs", "checkout at rust/crates"),
            (
                "rust/crates/yamaa/selftest/case-one/expected/out.csv",
                "checkout at rust/crates/yamaa/selftest/case-one/expected/out.csv",
            ),
            (
                "rust/crates/yamaa-adapters/schema/json/spec.schema.json",
                "checkout at rust/crates/yamaa-adapters/schema/json/spec.schema",
            ),
            ("vendor/pyo3-0.27.2/src/lib.rs", "files its .cargo-checksum"),
            ("vendor/pyo3-0.27.2/.cargo-checksum.json", "pyo3-0.27.2 has no"),
        ):
            with self.subTest(missing=missing):
                with self.assertRaisesRegex(ValueError, message):
                    self.check({k: v for k, v in entries.items() if k != missing})
        without = {k: v for k, v in entries.items() if "pyo3" not in k}
        with self.assertRaisesRegex(ValueError, "vendored crate pyo3-0.27.2"):
            self.check(without)
        with self.assertRaisesRegex(ValueError, "must not contain links"):
            self.check(entries, [("vendor/link", "pyo3-0.27.2")])
        for name in ("rust/../escape.rs", "rust//crates.rs", "rust\\crates.rs"):
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, "invalid member name"):
                    self.check(dict(entries, **{name: b""}))

    def test_r_files_beside_or_changed_from_the_bundle_refuse(self):
        entries = self.entries("r")
        for name, payload, message in (
            (
                "src/rust/crates/yamaa-core/tests/case.rs",
                b"//\n",
                "checkout at src/rust/crates/yamaa-core/tests",
            ),
            (
                "src/rust/crates/yamaa-python/src/lib.rs",
                b"//\n",
                "checkout at src/rust/crates/yamaa-python",
            ),
            (
                "src/rust/target/release/libyamaa_r.a",
                b"",
                "checkout at src/rust/target",
            ),
            ("src/rust/crates/yamaa-r/src/lib.rs", b"changed\n", "differs from the"),
            (
                "src/rust/crates/yamaa/selftest/case-one/spec.yaml",
                b"domain: CHANGED\n",
                "selftest/case-one/spec.yaml differs from the checkout",
            ),
            (
                "src/rust/crates/yamaa-adapters/schema/json/spec.schema.json",
                b"{}\r\n",
                "schema/json/spec.schema.json differs from the checkout",
            ),
            ("src/rust/vendor-config.toml", b"[source]\n", "differs from the"),
            # The Python bundle's manifest, with the wheels' stripping profile.
            (
                "src/rust/Cargo.toml",
                engine_bundle.workspace_manifest("python", self.workspace).encode(),
                "src/rust/Cargo.toml differs from the checkout",
            ),
            (
                "src/rust/Cargo.lock",
                self.canonical.encode(),
                "pyo3 0.27.2 is outside the dependency",
            ),
        ):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                self.check(dict(entries, **{name: payload}))
        for missing, message in (
            ("src/rust/crates/yamaa-r/src/lib.rs", "checkout at src/rust/crates"),
            (
                "src/rust/crates/yamaa/selftest/case-one/expected/out.csv",
                "checkout at src/rust/crates/yamaa/selftest/case-one/expected",
            ),
            (
                "src/rust/crates/yamaa-adapters/schema/schema_spec.yaml",
                "checkout at src/rust/crates/yamaa-adapters/schema/schema_spec",
            ),
            ("src/rust/vendor-config.toml", "checkout at src/rust/vendor-config"),
            ("src/rust/vendor.tar.xz", "the bundle is missing src/rust/vendor.tar.xz"),
        ):
            with self.subTest(missing=missing):
                with self.assertRaisesRegex(ValueError, message):
                    self.check({k: v for k, v in entries.items() if k != missing})
        # Files beside the bundle and outside src/rust are the R package's own.
        self.assertEqual(self.check(dict(entries, **{"src/entrypoint.c": b""}))[0], "r")

    def test_r_compressed_crates_follow_the_lock_and_their_records(self):
        crates = self.CRATES["r"]
        extendr = crates["extendr-0.9.0"]
        for vendored, links, message in (
            (
                self.vendored({**crates, "pyo3-0.27.2": {"Cargo.toml": b""}}),
                (),
                "vendored crate pyo3-0.27.2 differs from the lock's crates",
            ),
            (
                self.vendored({"parser-1.0.0": crates["parser-1.0.0"]}),
                (),
                "vendored crate extendr-0.9.0 differs from the lock's crates",
            ),
            (
                self.vendored(
                    {**crates, "extendr-0.9.0": {**extendr, "src/extra.rs": b""}},
                    ("extendr-0.9.0/src/extra.rs",),
                ),
                (),
                "extendr-0.9.0 differs from the files its .cargo-checksum.json",
            ),
            (
                {
                    k: v
                    for k, v in self.vendored(crates).items()
                    if k != "vendor/extendr-0.9.0/.cargo-checksum.json"
                },
                (),
                "extendr-0.9.0 has no .cargo-checksum.json",
            ),
            (self.vendored(crates), [("vendor/link", "extendr-0.9.0")], "links"),
            (
                {**self.vendored(crates), "other/file.rs": b""},
                (),
                "members must sit below vendor/",
            ),
            (
                {**self.vendored(crates), "vendor/../escape.rs": b""},
                (),
                "invalid member name",
            ),
        ):
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message):
                    self.check(self.entries("r", vendored, links))

    def test_an_r_package_whose_build_strips_refuses(self):
        # A stripping profile that the Makevars do not select is admitted. The
        # R bundle leaves out the wheels' one but keeps any other.
        manifest = self.workspace / "Cargo.toml"
        manifest.write_text(manifest.read_text() + SMALL)
        entries = self.entries("r")
        self.assertEqual(self.check(entries)[0], "r")
        for name in guard.R_MAKEVARS:
            small = entries[name].replace(b" --release ", b" --profile small ")
            self.assertNotEqual(small, entries[name])
            with self.subTest(name=name):
                with self.assertRaisesRegex(
                    ValueError, f"in the small profile, which {name} builds with"
                ):
                    self.check(dict(entries, **{name: small}))
                linked = entries[name] + b"PKG_LIBS += -Wl,-s\n"
                with self.assertRaisesRegex(
                    ValueError, f"{name}: -Wl,-s strips the shared object at R's"
                ):
                    self.check(dict(entries, **{name: linked}))
                with self.assertRaisesRegex(
                    ValueError, f"the R package is missing {name}"
                ):
                    self.check({k: v for k, v in entries.items() if k != name})
        manifest.write_text(
            manifest.read_text() + '\n[profile.release]\nstrip = "symbols"\n'
        )
        with self.assertRaisesRegex(
            ValueError, "the workspace manifest sets strip in the release profile"
        ):
            self.check(self.entries("r"))


DIST = '\n[profile.dist]\ninherits = "release"\nstrip = "symbols"\n'
# A stripping profile other than the wheels' one, which the R bundle keeps.
SMALL = '\n[profile.small]\ninherits = "release"\nstrip = "symbols"\n'


class RBuildSymbolsTests(unittest.TestCase):
    """The R package's build strips nothing, as CRAN's Repository Policy asks."""

    def makevars(self, selection="--release", environment=""):
        return (
            "$(RUST_LIB): FORCE\n"
            f"\tMAKEFLAGS= RUSTUP_AUTO_INSTALL=0 {environment}\\\n"
            f'\t  "$(CARGO)" build --locked --offline -j 2 {selection} -p yamaa-r\n'
        )

    def check(self, manifest="", config="", **makevars):
        guard.check_r_symbols(
            "yamaa", manifest, config, {"src/Makevars.in": self.makevars(**makevars)}
        )

    def test_the_checkout_r_build_selects_release_and_strips_nothing(self):
        templates = {
            name: (guard.R_PACKAGE / name).read_text(encoding="utf-8")
            for name in guard.R_MAKEVARS
        }
        for name, text in templates.items():
            self.assertEqual(guard.makevars_profile(name, text), "release")
        guard.check_r_symbols(
            "R/yamaa",
            engine_bundle.workspace_manifest("r"),
            engine_bundle.CARGO_CONFIG,
            templates,
        )

    def test_profiles_that_strip_nothing_are_admitted(self):
        for manifest in (
            "",
            DIST,
            "[profile.release]\nstrip = false\n",
            '[profile.release]\nstrip = "none"\n',
        ):
            with self.subTest(manifest=manifest):
                self.check(manifest)

    def test_a_build_that_strips_refuses(self):
        inherited = DIST + '[profile.r]\ninherits = "dist"\n'
        for arguments, message in (
            (
                {"manifest": '[profile.release]\nstrip = "debuginfo"\n'},
                "the workspace manifest sets strip in the release profile",
            ),
            (
                {"config": "[profile.release]\nstrip = true\n"},
                "the Cargo configuration sets strip in the release profile",
            ),
            (
                {"manifest": DIST, "selection": "--profile dist"},
                "in the dist profile, which src/Makevars.in builds with",
            ),
            ({"manifest": DIST, "selection": "--profile=dist"}, "the dist profile"),
            (
                {"manifest": inherited, "selection": "--profile r"},
                "in the dist profile, which the r profile of src/Makevars.in inherits",
            ),
            (
                {"environment": "CARGO_PROFILE_RELEASE_STRIP=symbols "},
                "the cargo build command sets strip",
            ),
            (
                {"selection": "--release --config profile.release.strip=true"},
                "the cargo build command sets strip",
            ),
        ):
            with self.subTest(arguments=arguments):
                with self.assertRaisesRegex(ValueError, message):
                    self.check(**arguments)

    def test_a_crlf_template_continues_its_cargo_command(self):
        # make reads CRLF as LF, so the setting on the first line reaches Cargo.
        makevars = self.makevars(environment="CARGO_PROFILE_RELEASE_STRIP=symbols ")
        with self.assertRaisesRegex(ValueError, "the cargo build command sets strip"):
            guard.check_r_symbols(
                "yamaa", "", "", {"src/Makevars.in": makevars.replace("\n", "\r\n")}
            )

    def test_one_cargo_build_selects_one_profile(self):
        with self.assertRaisesRegex(ValueError, "expected one cargo build command"):
            guard.check_r_symbols("yamaa", "", "", {"src/Makevars.in": "all:\n"})
        with self.assertRaisesRegex(ValueError, "selects more than one profile"):
            self.check(selection="--release --profile dist")
        self.assertEqual(guard.makevars_profile("x", self.makevars("")), "dev")


class RLinkStepTests(unittest.TestCase):
    """R's linker links the engine's static library into the shared object, so
    its link step, not Cargo, is where the Makevars could strip it."""

    def test_the_checkout_templates_strip_nothing_at_the_link_step(self):
        for name in guard.R_MAKEVARS:
            with self.subTest(name=name):
                guard.check_r_link(
                    name, (guard.R_PACKAGE / name).read_text(encoding="utf-8")
                )

    def test_comment_lines_other_commands_and_make_strip_are_admitted(self):
        guard.check_r_link(
            "src/Makevars.in",
            # make continues a comment line that ends in a backslash.
            "# Nothing strips the shared object: no strip, -s or -Wl,-S, \\\n"
            "  nor PKG_LIBS += -s\n"
            "PKG_LIBS = $(strip $(RUST_LIB)) -Wl,-soname,yamaa.so -Wl,-rpath,x\n"
            "define LIBS\n"
            "-lm\n"
            "endef\n"
            "rust_clean: $(SHLIB)\n"
            # A rule stays open through blank and comment lines, so the
            # tab-led lines after them are its recipe.
            "\n"
            "# Link the target.\n"
            "\t# strip $(SHLIB)\n"
            "\tln -s target link\n"
            # Other tools' commands that strip nothing.
            "\tinstall -d dir && ln -s target dir/link\n"
            "\tobjcopy -O binary $(SHLIB) yamaa.bin\n",
        )

    def test_each_way_to_strip_at_the_link_step_refuses(self):
        recipe = "rust_clean: $(SHLIB)\n\t"
        for text, problem in (
            # A command in a recipe: strip, or R's own $(STRIP_SHARED_LIB).
            (recipe + "strip --strip-debug $(SHLIB)\n", "a recipe runs strip;"),
            (recipe + "$(STRIP_SHARED_LIB) $(SHLIB)\n", "a recipe runs $(STRIP_"),
            # An option in PKG_LIBS or PKG_LDFLAGS, or in a variable they read.
            ("PKG_LIBS = $(RUST_LIB) -s\n", "-s strips"),
            ('PKG_LIBS = "$(RUST_LIB)" "-s"\n', "-s strips"),
            ("PKG_LIBS = $(RUST_LIB) -Wl,-s\n", "-Wl,-s strips"),
            ("PKG_LIBS = $(RUST_LIB) -Wl,--strip-all\n", "-Wl,--strip-all strips"),
            ("PKG_LDFLAGS = --strip-debug\n", "--strip-debug strips"),
            ("PKG_LDFLAGS = -Wl,-O1,-S\n", "-Wl,-O1,-S strips"),
            ("PKG_LIBS = $(RUST_LIB) -Xlinker -x\n", "-x strips"),
            ("LINK = -Wl,--discard-all\nPKG_LIBS = $(LINK)\n", "-Wl,--discard-all"),
            ("PKG_LIBS = $(RUST_LIB) \\\n\t-s\n", "-s strips"),
            ("override LDFLAGS += -Wl,-s\n", "-Wl,-s strips"),
            # make's $(strip ...) is admitted, but not an option inside it.
            ("PKG_LIBS = $(strip -s)\n", "-s strips"),
            ("PKG_LIBS = $(strip -Wl,-s)\n", "-Wl,-s strips"),
            # An option among a make function's arguments, or in quotes.
            ("PKG_LIBS = $(if $(DEBUG),,-s)\n", "$(DEBUG),,-s strips"),
            ("PKG_LIBS = $(if $(DEBUG),,-Wl,-x)\n", "$(DEBUG),,-Wl,-x strips"),
            ('PKG_LIBS = $(strip "-s")\n', "-s strips"),
            ('PKG_LIBS = $(shell echo "-Wl,-s")\n', "-Wl,-s strips"),
            ('PKG_LIBS = -Wl,"-s"\n', "-Wl,-s strips"),
            # A linker option strips whatever a recipe links too.
            (recipe + "$(SHLIB_LINK) -o $(SHLIB) -Wl,-s\n", "a recipe runs -Wl,-s;"),
            # Recipe tools that strip without the word strip.
            (recipe + "objcopy -g $(SHLIB)\n", "a recipe runs objcopy -g;"),
            (recipe + "$(OBJCOPY) -S $(SHLIB)\n", "a recipe runs $(OBJCOPY) -S;"),
            (
                recipe + "@x86_64-w64-mingw32-objcopy -x $(SHLIB)\n",
                "a recipe runs @x86_64-w64-mingw32-objcopy -x;",
            ),
            (recipe + "objcopy --discard-all $(SHLIB)\n", "a recipe runs objcopy --"),
            (recipe + "install -s $(SHLIB) yamaa.so\n", "a recipe runs install -s;"),
        ):
            with self.subTest(text=text):
                with self.assertRaisesRegex(
                    ValueError, "^" + re.escape(f"src/Makevars.in: {problem}")
                ):
                    guard.check_r_link("src/Makevars.in", text)
        with self.assertRaises(ValueError) as caught:
            guard.check_r_link("src/Makevars.in", "PKG_LIBS = $(RUST_LIB) -Wl,-s\n")
        self.assertEqual(
            str(caught.exception),
            "src/Makevars.in: -Wl,-s strips the shared object at R's link step; "
            "the R package strips nothing itself, as CRAN's Repository Policy "
            "asks (on Windows, R's own link flags strip every package DLL)",
        )

    def test_a_tab_led_line_is_a_recipe_only_in_an_open_rule(self):
        # make reads any other tab-led line as an ordinary line, so an
        # assignment there reaches R's link.
        for text, problem in (
            ("PKG_LIBS = $(RUST_LIB)\n\tPKG_LIBS += -Wl,-s\n", "-Wl,-s strips"),
            (
                "PKG_LIBS = $(RUST_LIB)\nifeq ($(OS),Darwin)\n"
                "\tPKG_LIBS += -Wl,-x\nendif\n",
                "-Wl,-x strips",
            ),
            ("\tPKG_LIBS += -s\nall: $(SHLIB)\n", "-s strips"),
            ("all: $(SHLIB)\nPKG_LIBS = -lm\n\tPKG_LIBS += -Wl,-s\n", "-Wl,-s strips"),
            # A define body is a variable's value, even after a line of it that
            # looks like a rule.
            ("define OPTS\n\t-Wl,-s\nendef\nPKG_LIBS = $(OPTS)\n", "-Wl,-s strips"),
            (
                "define OPTS\nx: y\n\t-s\nendef\nPKG_LIBS = $(strip $(OPTS))\n",
                "-s strips",
            ),
            # A rule that one branch opens is not open in the other, and a
            # target-specific variable opens none.
            (
                "ifeq ($(OS),Darwin)\nextra: $(SHLIB)\nelse\n\tPKG_LIBS += -s\nendif\n",
                "-s strips",
            ),
            ("$(SHLIB): X = 1\n\tPKG_LIBS += -s\n", "-s strips"),
            # make reads CRLF as LF, so the line continues.
            ("PKG_LIBS = $(RUST_LIB) -Xlinker \\\r\n\t-x\r\n", "-x strips"),
            # The shell runs the line after a recipe comment's backslash, and
            # joins a recipe's other lines without a space.
            (
                "rust_clean: $(SHLIB)\n\t# tidy \\\n\tstrip $(SHLIB)\n",
                "a recipe runs strip;",
            ),
            (
                "rust_clean: $(SHLIB)\n\t$(SHLIB_LINK) -Xlinker \\\n\t-x\n",
                "a recipe runs -x;",
            ),
            (
                "rust_clean: $(SHLIB)\n\t$(SHLIB_LINK) -Wl,\\\n\t-s\n",
                "a recipe runs -Wl,-s;",
            ),
        ):
            with self.subTest(text=text):
                with self.assertRaisesRegex(
                    ValueError, "^" + re.escape(f"src/Makevars.in: {problem}")
                ):
                    guard.check_r_link("src/Makevars.in", text)

    def test_comments_and_continuations_end_where_make_ends_them(self):
        for text, problem in (
            # A colon in a directive's trailing comment, as in a URL, opens no
            # rule, so the tab-led line after it is an assignment.
            (
                "ifeq ($(OS),Windows_NT)\n\texport FOO # see: https://example.org\n"
                "\tPKG_LIBS += -s\nendif\n",
                "-s strips",
            ),
            (
                "ifeq ($(OS),Windows_NT)\n\tunexport MAKEFLAGS # see: https://example.org\n"
                "\tPKG_LIBS += -s\nendif\n",
                "-s strips",
            ),
            ("$(info building) # note: x\n\tPKG_LIBS += -s\n", "-s strips"),
            # make continues a line only after an odd number of backslashes.
            ("# end \\\\\nPKG_LIBS += -Wl,-s\n", "-Wl,-s strips"),
            ("post: $(SHLIB)\n# end \\\\\n\tstrip $(SHLIB)\n", "a recipe runs strip;"),
            ("post: $(SHLIB)\n\techo done \\\\\nPKG_LIBS += -s\n", "-s strips"),
        ):
            with self.subTest(text=text):
                with self.assertRaisesRegex(
                    ValueError, "^" + re.escape(f"src/Makevars.in: {problem}")
                ):
                    guard.check_r_link("src/Makevars.in", text)
        # An escaped # and a rule line's comment leave its rule open, and three
        # backslashes continue a comment.
        guard.check_r_link(
            "src/Makevars.in",
            "link\\#1: $(SHLIB) # note: y\n\tln -s a b\n# end \\\\\\\nPKG_LIBS += -s\n",
        )

    def test_a_tool_behind_a_launcher_an_assignment_or_a_keyword_refuses(self):
        recipe = "rust_clean: $(SHLIB)\n\t"
        for text, problem in (
            # Each launcher, also after make's recipe prefix.
            (recipe + "env objcopy -g $(SHLIB)\n", "a recipe runs objcopy -g;"),
            (recipe + "@exec $(OBJCOPY) -S $(SHLIB)\n", "a recipe runs $(OBJCOPY) -S;"),
            (
                recipe + "time nohup nice command install -s $(SHLIB) yamaa.so\n",
                "a recipe runs install -s;",
            ),
            # One or more assignments, also after a launcher.
            (
                recipe + "FOO=1 BAR= install -s $(SHLIB) yamaa.so\n",
                "a recipe runs install -s;",
            ),
            (
                recipe + "env LC_ALL=C objcopy -x $(SHLIB)\n",
                "a recipe runs objcopy -x;",
            ),
            # Each shell keyword that a command follows.
            (
                recipe + "if objcopy -g $(SHLIB); then :; fi\n",
                "a recipe runs objcopy -g;",
            ),
            (
                recipe + "if true; then install -s $(SHLIB) yamaa.so; fi\n",
                "a recipe runs install -s;",
            ),
            (
                recipe + "if false; then :; elif objcopy -S $(SHLIB); then :; fi\n",
                "a recipe runs objcopy -S;",
            ),
            (
                recipe + "if false; then :; else objcopy -x $(SHLIB); fi\n",
                "a recipe runs objcopy -x;",
            ),
            (
                recipe + "while objcopy --discard-all $(SHLIB); do :; done\n",
                "a recipe runs objcopy --discard-all;",
            ),
            (
                recipe + "until install -s $(SHLIB) yamaa.so; do :; done\n",
                "a recipe runs install -s;",
            ),
            (
                recipe + "for f in $(SHLIB); do objcopy -g $$f; done\n",
                "a recipe runs objcopy -g;",
            ),
            (recipe + "! objcopy -g $(SHLIB)\n", "a recipe runs objcopy -g;"),
            (recipe + "{ objcopy -S $(SHLIB); }\n", "a recipe runs objcopy -S;"),
            (
                recipe + "( install -s $(SHLIB) yamaa.so )\n",
                "a recipe runs install -s;",
            ),
            # A subshell's ( joined to the tool still names it.
            (recipe + "(objcopy -x $(SHLIB))\n", "a recipe runs (objcopy -x;"),
        ):
            with self.subTest(text=text):
                with self.assertRaisesRegex(
                    ValueError, "^" + re.escape(f"src/Makevars.in: {problem}")
                ):
                    guard.check_r_link("src/Makevars.in", text)
        # The same words before another command, or before these tools without
        # their strip options, are admitted.
        guard.check_r_link(
            "src/Makevars.in",
            recipe + "env FOO=1 ln -s target link\n"
            "\tif test -d dir; then install -d dir; fi\n"
            "\t{ objcopy -O binary $(SHLIB) yamaa.bin; }\n",
        )

    def test_a_single_ampersand_ends_a_shell_command(self):
        recipe = "rust_clean: $(SHLIB)\n\t"
        for text, problem in (
            (recipe + "true & objcopy -g $(SHLIB)\n", "a recipe runs objcopy -g;"),
            (
                recipe + "objcopy -O binary $(SHLIB) x&install -s $(SHLIB) yamaa.so\n",
                "a recipe runs install -s;",
            ),
            # The & of a redirection ends no command, so an option after it
            # still counts.
            (recipe + "objcopy 2>&1 -g $(SHLIB)\n", "a recipe runs objcopy -g;"),
            (
                recipe + "install &>install.log -s $(SHLIB) yamaa.so\n",
                "a recipe runs install -s;",
            ),
        ):
            with self.subTest(text=text):
                with self.assertRaisesRegex(
                    ValueError, "^" + re.escape(f"src/Makevars.in: {problem}")
                ):
                    guard.check_r_link("src/Makevars.in", text)
        # The -s after a single & belongs to the next command, as after &&.
        guard.check_r_link(
            "src/Makevars.in", recipe + "install -d dir & ln -s target dir/link\n"
        )


if __name__ == "__main__":
    unittest.main()
