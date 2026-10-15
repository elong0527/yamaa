"""Regression tests for the sdist build backend's workspace restore."""

import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pytest
from python import yamaa_build_backend as _BACKEND_MODULE
from tools.packaging import check_cargo_lock as guard

backend = _BACKEND_MODULE
preserve_workspace = _BACKEND_MODULE.preserve_workspace


def _make_workspace(root: Path) -> Path:
    workspace = root / "rust"
    (workspace / "crates" / "foo" / "src").mkdir(parents=True)
    (workspace / "Cargo.toml").write_bytes(b'[workspace]\nmembers = ["crates/foo"]\n')
    (workspace / "Cargo.lock").write_bytes(b"lock-bytes")
    (workspace / "rust-toolchain.toml").write_bytes(b"toolchain-bytes")
    (workspace / "crates" / "foo" / "Cargo.toml").write_bytes(
        b'[package]\nname = "foo"\n'
    )
    (workspace / "crates" / "foo" / "src" / "lib.rs").write_bytes(b"pub fn f() {}\n")
    return workspace


def _make_archive(path: Path, entries: list[tuple[str, bytes]]) -> Path:
    with tarfile.open(path, "w:gz") as target:
        for name, payload in entries:
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            target.addfile(info, io.BytesIO(payload))
    return path


def _archive_payloads(path: Path) -> dict[str, bytes]:
    with tarfile.open(path, "r:gz") as source:
        return {
            member.name: source.extractfile(member).read()
            for member in source.getmembers()
            if member.isfile()
        }


_BASE_ENTRIES = [
    ("pkg-0.1/rust/Cargo.toml", b"stale manifest\n"),
    ("pkg-0.1/rust/Cargo.lock", b"lock-bytes"),
]


def test_member_sources_restored_when_manifest_already_present(
    tmp_path: Path,
) -> None:
    # A member whose manifest survived in the sdist but whose sources did
    # not must still have every source restored.
    workspace = _make_workspace(tmp_path)
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n')],
    )
    preserve_workspace(archive, workspace)
    payloads = _archive_payloads(archive)
    assert payloads["pkg-0.1/rust/crates/foo/src/lib.rs"] == b"pub fn f() {}\n"


def test_tampered_member_source_detected_when_manifest_already_present(
    tmp_path: Path,
) -> None:
    # An altered member source must raise even when the member manifest is
    # already in the archive.
    workspace = _make_workspace(tmp_path)
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n'),
            (
                "pkg-0.1/rust/crates/foo/src/lib.rs",
                b"pub fn f() { /* tampered */ }\n",
            ),
        ],
    )
    with pytest.raises(
        ValueError, match="source archive changed a restored member source"
    ):
        preserve_workspace(archive, workspace)


def test_intact_member_archive_preserved(tmp_path: Path) -> None:
    # An intact archive keeps every entry; the workspace manifest is still
    # replaced with the local canonical copy.
    workspace = _make_workspace(tmp_path)
    (workspace / "crates/foo/build.rs").write_bytes(b"fn main() {}\n")
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [
            ("pkg-0.1/rust/rust-toolchain.toml", b"toolchain-bytes"),
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n'),
            ("pkg-0.1/rust/crates/foo/build.rs", b"fn main() {}\n"),
            ("pkg-0.1/rust/crates/foo/src/lib.rs", b"pub fn f() {}\n"),
        ],
    )
    preserve_workspace(archive, workspace)
    payloads = _archive_payloads(archive)
    assert (
        payloads["pkg-0.1/rust/Cargo.toml"] == (workspace / "Cargo.toml").read_bytes()
    )
    assert payloads["pkg-0.1/rust/crates/foo/src/lib.rs"] == b"pub fn f() {}\n"
    assert payloads["pkg-0.1/rust/crates/foo/build.rs"] == b"fn main() {}\n"


@pytest.mark.parametrize("tampered", [False, True])
def test_readstat_native_sources_and_license_are_verified_and_restored(
    tmp_path: Path, tampered: bool
) -> None:
    workspace = _make_workspace(tmp_path)
    adapter = workspace / "crates/yamaa-adapters"
    (workspace / "crates/foo").rename(adapter)
    (workspace / "Cargo.toml").write_bytes(
        b'[workspace]\nmembers = ["crates/yamaa-adapters"]\n'
    )
    vendor = adapter / "vendor/readstat"
    vendor.mkdir(parents=True)
    (vendor / "reader.c").write_bytes(b"int reader(void) { return 0; }\n")
    (vendor / "reader.h").write_bytes(b"int reader(void);\n")
    (vendor / "LICENSE").write_bytes(b"independent-license-bytes\n")
    entries = list(_BASE_ENTRIES)
    native_path = "pkg-0.1/rust/crates/yamaa-adapters/vendor/readstat/reader.c"
    if tampered:
        entries.append((native_path, b"altered-native-source\n"))
    archive = _make_archive(tmp_path / "sdist.tar.gz", entries)
    if tampered:
        with pytest.raises(ValueError, match="changed a restored member source"):
            preserve_workspace(archive, workspace)
    else:
        preserve_workspace(archive, workspace)
        payloads = _archive_payloads(archive)
        assert payloads[native_path] == (vendor / "reader.c").read_bytes()
        assert (
            payloads[native_path.removesuffix("reader.c") + "reader.h"]
            == (vendor / "reader.h").read_bytes()
        )
        assert (
            payloads[native_path.removesuffix("reader.c") + "LICENSE"]
            == (vendor / "LICENSE").read_bytes()
        )


def test_readstat_vendor_parent_symlink_cannot_archive_external_sources(tmp_path):
    workspace = _make_workspace(tmp_path)
    adapter = workspace / "crates/yamaa-adapters"
    (workspace / "crates/foo").rename(adapter)
    external = tmp_path / "external/readstat"
    external.mkdir(parents=True)
    (external / "reader.c").write_bytes(b"external-native-source\n")
    try:
        (adapter / "vendor").symlink_to(external.parent, target_is_directory=True)
    except OSError:
        pytest.skip("directory symlinks unavailable")
    with pytest.raises(ValueError, match="vendor sources must not be symlinks"):
        list(_BACKEND_MODULE.member_sources(adapter))


@pytest.mark.parametrize("manifest_present", [False, True])
def test_missing_member_build_script_restored(
    tmp_path: Path, manifest_present: bool
) -> None:
    workspace = _make_workspace(tmp_path)
    (workspace / "crates/foo/build.rs").write_bytes(b"fn main() {}\n")
    entries = _BASE_ENTRIES + [
        ("pkg-0.1/rust/crates/foo/src/lib.rs", b"pub fn f() {}\n")
    ]
    if manifest_present:
        entries.append(
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n')
        )
    archive = _make_archive(tmp_path / "sdist.tar.gz", entries)

    preserve_workspace(archive, workspace)

    assert _archive_payloads(archive)["pkg-0.1/rust/crates/foo/build.rs"] == (
        b"fn main() {}\n"
    )


@pytest.mark.parametrize(
    "entry_type", [tarfile.REGTYPE, tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.DIRTYPE]
)
def test_changed_member_build_script_rejected(
    tmp_path: Path, entry_type: bytes
) -> None:
    workspace = _make_workspace(tmp_path)
    (workspace / "crates/foo/build.rs").write_bytes(b"fn main() {}\n")
    archive = _make_archive(
        tmp_path / "sdist.tar.gz",
        _BASE_ENTRIES
        + [
            ("pkg-0.1/rust/crates/foo/Cargo.toml", b'[package]\nname = "foo"\n'),
            ("pkg-0.1/rust/crates/foo/src/lib.rs", b"pub fn f() {}\n"),
        ],
    )
    with tarfile.open(archive, "r:gz") as source:
        entries = [(entry, source.extractfile(entry).read()) for entry in source]
    with tarfile.open(archive, "w:gz") as target:
        for entry, payload in entries:
            target.addfile(entry, io.BytesIO(payload))
        entry = tarfile.TarInfo("pkg-0.1/rust/crates/foo/build.rs")
        entry.type = entry_type
        entry.linkname = "pkg-0.1/rust/crates/foo/src/lib.rs"
        payload = b"fn main() { /* tampered */ }\n"
        if entry.isfile():
            entry.size = len(payload)
        target.addfile(entry, io.BytesIO(payload) if entry.isfile() else None)
    original = archive.read_bytes()

    with pytest.raises(
        ValueError, match="source archive changed a restored member source"
    ):
        preserve_workspace(archive, workspace)

    assert archive.read_bytes() == original


@pytest.mark.parametrize("target_exists", [False, True])
def test_local_member_build_script_symlink_rejected(
    tmp_path: Path, target_exists: bool
) -> None:
    workspace = _make_workspace(tmp_path)
    target = tmp_path / "outside.rs"
    if target_exists:
        target.write_bytes(b"fn main() {}\n")
    try:
        (workspace / "crates/foo/build.rs").symlink_to(target)
    except OSError as error:
        pytest.skip(f"build script symlink witness is unavailable: {error}")
    archive = _make_archive(tmp_path / "sdist.tar.gz", _BASE_ENTRIES)
    original = archive.read_bytes()

    with pytest.raises(
        ValueError, match="restored workspace sources must not be symlinks"
    ):
        preserve_workspace(archive, workspace)

    assert archive.read_bytes() == original


def write_archive(path, values):
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in values:
            member = tarfile.TarInfo(name)
            member.size = len(payload)
            archive.addfile(member, io.BytesIO(payload))


class PythonSourceWorkspaceTests(unittest.TestCase):
    def patch_fixture(self, root):
        workspace = root / "rust"
        workspace.mkdir()
        (workspace / "Cargo.toml").write_text(
            "[workspace]\nmembers = []\n[patch.crates-io]\n"
            'witness = {path = "vendor/witness"}\n'
        )
        (workspace / "Cargo.lock").write_bytes(b"canonical lock")
        (workspace / "rust-toolchain.toml").write_bytes(b"canonical toolchain")
        package = workspace / "vendor/witness"
        for name, payload in {
            "Cargo.toml": b'[package]\nname = "witness"\nversion = "0.1.0"\n',
            "src/lib.rs": b"pub fn witness() {}\n",
            "LICENSE": b"two upstream authors\n",
            "LICENSES/first-MIT": b"first notice\n",
            "LICENSES/second-MIT": b"second notice\n",
            "README.md": b"released readme\n",
            "PROVENANCE.md": b"release plus narrow source patch\n",
            "compact-flow-pairs.patch": b"independent source patch\n",
        }.items():
            source = package / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_bytes(payload)
        archive = root / "yamaa.tar.gz"
        write_archive(
            archive,
            [
                ("yamaa/rust/Cargo.toml", (workspace / "Cargo.toml").read_bytes()),
                ("yamaa/rust/Cargo.lock", (workspace / "Cargo.lock").read_bytes()),
            ],
        )
        return workspace, package, archive

    def test_pruned_local_patch_sources_and_both_notices_are_restored_and_guarded(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, _, archive = self.patch_fixture(Path(directory))
            sources = [
                (source.relative_to(workspace).as_posix(), source.read_bytes())
                for source in backend.local_patch_sources(workspace)
            ]
            lock = (workspace / "Cargo.lock").read_bytes()
            manifest = (workspace / "Cargo.toml").read_bytes()
            with self.assertRaisesRegex(ValueError, "regular local Cargo patch"):
                guard.check_archive(archive, lock, manifest, patch_sources=sources)
            backend.preserve_workspace(archive, workspace)
            guard.check_archive(archive, lock, manifest, patch_sources=sources)
            with tarfile.open(archive) as restored:
                for relative, payload in sources:
                    self.assertEqual(
                        restored.extractfile("yamaa/rust/" + relative).read(), payload
                    )

    def test_changed_local_patch_refuses_atomically_and_archive_guard_refuses_it(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, _, archive = self.patch_fixture(Path(directory))
            source = "vendor/witness/src/lib.rs"
            values = [
                ("yamaa/rust/Cargo.toml", (workspace / "Cargo.toml").read_bytes()),
                ("yamaa/rust/Cargo.lock", (workspace / "Cargo.lock").read_bytes()),
                ("yamaa/rust/" + source, b"changed parser source"),
            ]
            write_archive(archive, values)
            original = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "changed a local Cargo patch"):
                backend.preserve_workspace(archive, workspace)
            self.assertEqual(archive.read_bytes(), original)
            with self.assertRaisesRegex(ValueError, "differs from canonical bytes"):
                guard.check_archive(
                    archive,
                    (workspace / "Cargo.lock").read_bytes(),
                    (workspace / "Cargo.toml").read_bytes(),
                    patch_sources=[(source, (workspace / source).read_bytes())],
                )

    def test_local_patch_symlinks_and_escaping_declarations_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, package, _ = self.patch_fixture(Path(directory))
            notice = package / "LICENSES/second-MIT"
            notice.unlink()
            try:
                notice.symlink_to(package / "LICENSES/first-MIT")
            except OSError as error:
                self.skipTest(f"local patch symlink control is unavailable: {error}")
            with self.assertRaisesRegex(ValueError, "must not be symlinks"):
                list(backend.local_patch_sources(workspace))
            for path in ("../outside", "vendor/../outside", "/outside"):
                (workspace / "Cargo.toml").write_text(
                    "[workspace]\nmembers = []\n[patch.crates-io]\n"
                    f'witness = {{path = "{path}"}}\n'
                )
                with self.assertRaisesRegex(ValueError, "vendor/package paths"):
                    list(backend.local_patch_sources(workspace))

    def test_extra_local_patch_build_script_cannot_enter_a_source_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, _, archive = self.patch_fixture(Path(directory))
            sources = [
                (source.relative_to(workspace).as_posix(), source.read_bytes())
                for source in backend.local_patch_sources(workspace)
            ]
            with tarfile.open(archive) as original:
                values = [
                    (entry.name, original.extractfile(entry).read())
                    for entry in original
                ]
            values.append(("yamaa/rust/vendor/witness/build.rs", b"fn main() {}"))
            write_archive(archive, values)
            original = archive.read_bytes()
            with self.assertRaisesRegex(
                ValueError, "unexpected local Cargo patch file"
            ):
                backend.preserve_workspace(archive, workspace)
            self.assertEqual(archive.read_bytes(), original)
            with self.assertRaisesRegex(
                ValueError, "unexpected local Cargo patch file"
            ):
                guard.check_archive(
                    archive,
                    (workspace / "Cargo.lock").read_bytes(),
                    (workspace / "Cargo.toml").read_bytes(),
                    patch_sources=sources,
                )

    def test_archive_local_patch_directory_links_refuse_before_restoration(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, _, archive = self.patch_fixture(Path(directory))
            sources = [
                (source.relative_to(workspace).as_posix(), source.read_bytes())
                for source in backend.local_patch_sources(workspace)
            ]
            with tarfile.open(archive) as original:
                values = [
                    (entry.name, original.extractfile(entry).read())
                    for entry in original
                ]
            for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                with tarfile.open(archive, "w:gz") as target:
                    for name, payload in values:
                        entry = tarfile.TarInfo(name)
                        entry.size = len(payload)
                        target.addfile(entry, io.BytesIO(payload))
                    link = tarfile.TarInfo("yamaa/rust/vendor/witness")
                    link.type = kind
                    link.linkname = "yamaa/rust/another-package"
                    target.addfile(link)
                original = archive.read_bytes()
                with self.assertRaisesRegex(ValueError, "must not contain links"):
                    backend.preserve_workspace(archive, workspace)
                self.assertEqual(archive.read_bytes(), original)
                with self.assertRaisesRegex(ValueError, "must not contain links"):
                    guard.check_archive(
                        archive,
                        (workspace / "Cargo.lock").read_bytes(),
                        (workspace / "Cargo.toml").read_bytes(),
                        patch_sources=sources,
                    )

    def compile_fixture(self, root, declared=True):
        workspace = root / "rust"
        name = "benchmarks/original/expected/out.csv"
        original = root / name
        original.parent.mkdir(parents=True)
        original.write_bytes(b"ID\n1\n")
        manifest = workspace / "qualification/native-source-fixtures.json"
        manifest.parent.mkdir(parents=True)
        manifest.write_text(
            json.dumps(
                {
                    "format": "yamaa-native-source-fixtures/1",
                    "files": [name] if declared else [],
                }
            )
        )
        source = workspace / "crates/engine/tests/original.rs"
        source.parent.mkdir(parents=True)
        return workspace, source, original, name

    def test_ordinary_compile_literals_admit_all_macro_delimiters(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    with self.subTest(macro=macro, opening=opening):
                        source.write_text(
                            "fn main() { let _ = "
                            + macro
                            + "!"
                            + opening
                            + json.dumps("../../../../" + name)
                            + closing
                            + "; }\n"
                        )
                        selected = backend.native_test_fixtures(workspace)
                        self.assertEqual(selected[1], (name, original))
                        self.assertEqual(selected[1][1].read_bytes(), b"ID\n1\n")

    def test_brace_bracket_compile_literals_refuse_absolute_and_undeclared_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(
                Path(directory), declared=False
            )
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("[", "]"), ("{", "}")):
                    for path, reason in (
                        (str(original.resolve()), "unescaped relative paths"),
                        ("../../../../" + name, "not declared"),
                    ):
                        with self.subTest(macro=macro, opening=opening, path=path):
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + json.dumps(path)
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(ValueError, reason):
                                backend.native_test_fixtures(workspace)

    def test_raw_compile_literals_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    for hashes in ("", "#", "###"):
                        with self.subTest(macro=macro, opening=opening, hashes=hashes):
                            literal = (
                                "r"
                                + hashes
                                + '"'
                                + str(original.resolve())
                                + '"'
                                + hashes
                            )
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + literal
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(
                                ValueError, "ordinary string literals"
                            ):
                                backend.native_test_fixtures(workspace)

    def test_dynamic_and_nonliteral_compile_arguments_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, _, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    for value in (
                        'concat!("../../../../", "benchmarks/original/expected/out.csv")',
                        'env!("YAMAA_GUARD_CONTROL_PATH")',
                        "SOURCE_PATH",
                    ):
                        with self.subTest(macro=macro, opening=opening, value=value):
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + value
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(
                                ValueError, "ordinary string literals"
                            ):
                                backend.native_test_fixtures(workspace)

    def test_comment_separated_compile_tokens_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for gap in ("/* gap */", "/* outer /* inner */ gap */", "// gap\n"):
                    for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                        for before_bang in (True, False):
                            with self.subTest(
                                macro=macro,
                                gap=gap,
                                opening=opening,
                                before_bang=before_bang,
                            ):
                                tokens = (
                                    macro + gap + "!" + opening
                                    if before_bang
                                    else macro + "!" + gap + opening
                                )
                                source.write_text(
                                    "fn main() { let _ = "
                                    + tokens
                                    + json.dumps(str(original.resolve()))
                                    + closing
                                    + "; }\n"
                                )
                                with self.assertRaisesRegex(
                                    ValueError, "ordinary string literals"
                                ):
                                    backend.native_test_fixtures(workspace)

    def test_spaced_bang_arguments_follow_literal_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                with self.subTest(macro=macro, kind="ordinary"):
                    source.write_text(
                        "fn main() { let _ = "
                        + macro
                        + " ! ("
                        + json.dumps("../../../../" + name)
                        + "); }\n"
                    )
                    selected = backend.native_test_fixtures(workspace)
                    self.assertEqual(selected[1], (name, original))
                for value in (
                    'r"' + str(original.resolve()) + '"',
                    'concat!("../../../../", "benchmarks/original/expected/out.csv")',
                ):
                    with self.subTest(macro=macro, value=value):
                        source.write_text(
                            "fn main() { let _ = " + macro + " ! (" + value + "); }\n"
                        )
                        with self.assertRaisesRegex(
                            ValueError, "ordinary string literals"
                        ):
                            backend.native_test_fixtures(workspace)

    def test_absolute_and_drive_compile_paths_cannot_read_the_original_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            for path in (
                str(original.resolve()),
                "C:/original/out.csv",
                r"C:\original\out.csv",
            ):
                with self.subTest(path=path):
                    source.write_text(
                        f"const INPUT: &str = include_str!({json.dumps(path)});\n"
                    )
                    with self.assertRaisesRegex(ValueError, "unescaped relative paths"):
                        backend.native_test_fixtures(workspace)

    def test_compile_symlink_alias_is_not_replaced_by_its_declared_regular_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            alias = root / "benchmarks/alias"
            try:
                alias.symlink_to(original.parent, target_is_directory=True)
            except OSError as error:
                self.skipTest(f"compile-input symlink witness is unavailable: {error}")
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                'const INPUT: &str = include_str!("../../../../benchmarks/alias/out.csv");\n'
            )
            with self.assertRaisesRegex(
                ValueError, "compile fixtures must not be symlinks"
            ):
                backend.native_test_fixtures(workspace)

    def test_external_compile_fixture_must_be_declared_before_building_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            source.write_text(
                "const ORIGINAL: &str = include_str!(\n"
                '    "../../../../benchmarks/original/expected/out.csv"\n);\n'
            )
            manifest.write_text(
                json.dumps({"format": "yamaa-native-source-fixtures/1", "files": []})
            )
            with self.assertRaisesRegex(ValueError, "compile fixture is not declared"):
                backend.native_test_fixtures(workspace)
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = backend.native_test_fixtures(workspace)
            self.assertEqual(selected[1][0], name)
            self.assertEqual(selected[1][1].read_bytes(), b"ID\n1\n")

    def test_native_fixture_manifest_selects_exact_regular_original_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/independent/expected/document.xml"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"independent fixed-time document\n")
            (original.parent / "unselected.csv").write_bytes(b"not selected\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = backend.native_test_fixtures(workspace)
            self.assertEqual(
                [name for name, _ in selected],
                ["rust/qualification/native-source-fixtures.json", name],
            )
            self.assertEqual(
                selected[1][1].read_bytes(), b"independent fixed-time document\n"
            )
            original.unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                backend.native_test_fixtures(workspace)

    def test_native_fixture_manifest_refuses_ambiguous_or_unselected_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory) / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            for files in (
                ["../private"],
                ["/benchmarks/private"],
                ["benchmarks/../private"],
                ["benchmarks/a\\b"],
                ["benchmarks/a:stream"],
                ["benchmarks/a", "benchmarks/a"],
            ):
                with self.subTest(files=files):
                    manifest.write_text(
                        json.dumps(
                            {"format": "yamaa-native-source-fixtures/1", "files": files}
                        )
                    )
                    with self.assertRaisesRegex(ValueError, "fixture path"):
                        backend.native_test_fixtures(workspace)

    def test_source_fixture_restore_preserves_bytes_and_refuses_changed_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            workspace.mkdir()
            manifest = b'[workspace]\nmembers = []\nresolver = "2"\n'
            lock = b"version = 4\n"
            toolchain = b'[toolchain]\nchannel = "1.90.0"\n'
            for name, payload in (
                ("Cargo.toml", manifest),
                ("Cargo.lock", lock),
                ("rust-toolchain.toml", toolchain),
            ):
                (workspace / name).write_bytes(payload)
            truth = root / "original.xml"
            truth.write_bytes(b"independently committed truth\n")
            archive = root / "yamaa.tar.gz"
            values = [
                ("yamaa/rust/Cargo.toml", manifest),
                ("yamaa/rust/Cargo.lock", lock),
            ]
            selected = [("benchmarks/independent/expected/document.xml", truth)]
            write_archive(archive, values)
            backend.preserve_workspace(archive, workspace, selected)
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile(
                        "yamaa/benchmarks/independent/expected/document.xml"
                    ).read(),
                    truth.read_bytes(),
                )
            write_archive(
                archive,
                values
                + [
                    (
                        "yamaa/benchmarks/independent/expected/document.xml",
                        b"changed truth\n",
                    )
                ],
            )
            before = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "changed a native test fixture"):
                backend.preserve_workspace(archive, workspace, selected)
            self.assertEqual(archive.read_bytes(), before)
            self.assertEqual(truth.read_bytes(), b"independently committed truth\n")

    def test_pruned_archive_is_restored_and_resolves_without_rewriting_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            workspace.mkdir()
            manifest = b'[workspace]\nmembers = ["crates/python", "crates/r"]\nresolver = "2"\n'
            (workspace / "Cargo.toml").write_bytes(manifest)
            toolchain = b'[toolchain]\nchannel = "1.90.0"\n'
            (workspace / "rust-toolchain.toml").write_bytes(toolchain)
            for member in ("python", "r"):
                crate = workspace / "crates" / member
                (crate / "src").mkdir(parents=True)
                (crate / "Cargo.toml").write_text(
                    f'[package]\nname = "witness-{member}"\nversion = "0.1.0"\nedition = "2021"\n'
                )
                (crate / "src/lib.rs").write_text("pub fn unchanged() {}\n")
            subprocess.run(
                ["cargo", "generate-lockfile", "--offline"],
                cwd=workspace,
                check=True,
                capture_output=True,
            )
            lock = (workspace / "Cargo.lock").read_bytes()
            archive = root / "yamaa.tar.gz"
            values = [
                ("yamaa/rust/Cargo.toml", manifest.replace(b', "crates/r"', b"")),
                ("yamaa/rust/Cargo.lock", lock),
                ("yamaa/pyproject.toml", b"independent Python metadata\n"),
            ]
            values += [
                (
                    f"yamaa/rust/crates/python/{name}",
                    (workspace / "crates/python" / name).read_bytes(),
                )
                for name in ("Cargo.toml", "src/lib.rs")
            ]
            write_archive(archive, values)
            with self.assertRaisesRegex(ValueError, "manifest differs"):
                guard.check_archive(archive, lock, manifest)
            for name in (
                "target/generated.rs",
                "__pycache__/cache.pyc",
                "Cargo.lock",
                "private.txt",
                "local.rs",
                "src/local-notes.txt",
            ):
                path = workspace / "crates/r" / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(b"generated")
            backend.preserve_workspace(archive, workspace)
            guard.check_archive(archive, lock, manifest, toolchain)
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.toml").read(), manifest
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.lock").read(), lock
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/rust-toolchain.toml").read(),
                    toolchain,
                )
                self.assertEqual(
                    result.extractfile("yamaa/pyproject.toml").read(), values[2][1]
                )
                self.assertEqual(
                    result.extractfile("yamaa/rust/crates/r/src/lib.rs").read(),
                    (workspace / "crates/r/src/lib.rs").read_bytes(),
                )
                self.assertEqual(len(result.getnames()), len(values) + 3)
                result.extractall(root / "unpacked", filter="data")
            restored = root / "unpacked/yamaa/rust"
            subprocess.run(
                ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
                cwd=restored,
                check=True,
                capture_output=True,
            )
            self.assertEqual((restored / "Cargo.lock").read_bytes(), lock)

    def test_atomic_replacement_closes_archive_handles_first(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = b"[workspace]\nmembers = []\n"
            (root / "Cargo.toml").write_bytes(manifest)
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"pruned"),
                    ("yamaa/rust/Cargo.lock", b"canonical"),
                    ("yamaa/held.txt", b"unchanged payload"),
                ],
            )
            handles = []
            original_open = tarfile.open
            original_replace = backend.os.replace
            replacements = []

            def tracked_open(*args, **kwargs):
                handle = original_open(*args, **kwargs)
                handles.append(handle)
                return handle

            def replace_closed(source, destination):
                self.assertEqual(len(handles), 2)
                self.assertTrue(
                    all(handle.closed and handle.fileobj.closed for handle in handles)
                )
                replacements.append(destination)
                original_replace(source, destination)

            with (
                patch.object(backend.tarfile, "open", side_effect=tracked_open),
                patch.object(backend.os, "replace", side_effect=replace_closed),
            ):
                backend.preserve_workspace(archive, root)
            self.assertEqual(replacements, [archive])
            with tarfile.open(archive) as result:
                self.assertEqual(
                    result.extractfile("yamaa/rust/Cargo.toml").read(), manifest
                )
                self.assertEqual(
                    result.extractfile("yamaa/held.txt").read(), b"unchanged payload"
                )
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])

    def test_external_source_symlink_refuses_without_replacing_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = b'[workspace]\nmembers = ["crates/r"]\n'
            (root / "Cargo.toml").write_bytes(manifest)
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            member = root / "crates/r"
            (member / "src").mkdir(parents=True)
            (member / "Cargo.toml").write_bytes(b"member")
            (root / "private.rs").write_bytes(b"outside member")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"pruned"),
                    ("yamaa/rust/Cargo.lock", b"canonical"),
                ],
            )
            original = archive.read_bytes()
            try:
                (member / "src/lib.rs").symlink_to(root / "private.rs")
            except OSError as error:
                self.skipTest(f"source symlink witness is unavailable: {error}")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                backend.preserve_workspace(archive, root)
            self.assertEqual(archive.read_bytes(), original)
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])

    def test_source_directory_symlink_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            member = root / "crates/r"
            (member / "src").mkdir(parents=True)
            (member / "Cargo.toml").write_bytes(b"member")
            (root / "outside").mkdir()
            (root / "outside/private.rs").write_bytes(b"outside member")
            try:
                (member / "src/linked").symlink_to(
                    root / "outside", target_is_directory=True
                )
            except OSError as error:
                self.skipTest(f"directory symlink witness is unavailable: {error}")
            with self.assertRaisesRegex(ValueError, "symlinks"):
                list(backend.member_sources(member))

    def test_invalid_archive_roots_refuse_without_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_bytes(b"[workspace]\nmembers = []\n")
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            for package in (
                "/yamaa",
                "../yamaa",
                "./yamaa",
                "yamaa/..",
                "C:",
                "C:\\yamaa",
            ):
                with self.subTest(package=package):
                    write_archive(
                        archive,
                        [
                            (f"{package}/rust/Cargo.toml", b"pruned"),
                            (f"{package}/rust/Cargo.lock", b"canonical"),
                        ],
                    )
                    original = archive.read_bytes()
                    with self.assertRaisesRegex(ValueError, "invalid package root"):
                        backend.preserve_workspace(archive, root)
                    self.assertEqual(archive.read_bytes(), original)

    def test_contradictory_archive_refuses_without_replacing_original(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\nmembers = []\n")
            (root / "rust-toolchain.toml").write_bytes(b"toolchain")
            (root / "Cargo.lock").write_bytes(b"canonical")
            archive = root / "yamaa.tar.gz"
            write_archive(
                archive,
                [
                    ("yamaa/rust/Cargo.toml", b"manifest"),
                    ("yamaa/rust/Cargo.lock", b"other"),
                ],
            )
            original = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "changed"):
                backend.preserve_workspace(archive, root)
            self.assertEqual(archive.read_bytes(), original)
            self.assertEqual(list(root.glob("*.tar.gz")), [archive])
