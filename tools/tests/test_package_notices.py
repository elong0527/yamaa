"""Each built package carries every package notice at its own location."""

import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from tools.packaging import check_package_notices as packages
from tools.packaging import generate_component_inventory as inventory


class PackageNotices(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "python").mkdir()
        (self.root / "tools/packaging").mkdir(parents=True)
        (self.root / "tools/packaging/release.toml").write_text('version = "0.4.0"\n')
        # The toolchain that builds the release binaries.
        (self.root / "rust").mkdir()
        (self.root / "rust/rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.99.1"\n'
        )
        (self.root / "python/pyproject.toml").write_text(
            '[project]\nversion = "0.4.0"\n'
            'license-files = ["LICENSE.txt", "LICENSE-THIRD-PARTY.txt", '
            '"COMPONENTS.tsv"]\n'
        )
        # The hosts share the project license, but their third-party notices
        # and component inventories list their own crates. The wheels'
        # inventory also names the standard library that they link, which the
        # Python source archive's leaves out.
        project = {"LICENSE.txt": b"project\n"}
        self.notices = dict(
            project,
            **{
                "LICENSE-THIRD-PARTY.txt": b"Crate: pyo3 0.27.2\n",
                "COMPONENTS.tsv": b"name\npyo3\t0.27.2\nrust-std\t1.99.1\n",
            },
        )
        self.sdist_notices = dict(
            self.notices, **{"COMPONENTS.tsv": b"name\npyo3\t0.27.2\n"}
        )
        self.r_notices = dict(
            project,
            **{
                "LICENSE-THIRD-PARTY.txt": b"Crate: extendr-api 0.9.0\n",
                "COMPONENTS.tsv": b"name\nextendr-api\t0.9.0\n",
            },
        )
        # The R package's configure step appends the standard library that
        # rustc links to the inventory that R installs, so an R binary package
        # built with the pinned toolchain names that toolchain's.
        self.r_binary_notices = dict(
            self.r_notices,
            **{"COMPONENTS.tsv": self.installed_inventory("1.99.1")},
        )
        # The command line keeps only its own notices and inventory beside
        # its crate, an inventory that names the standard library that its
        # binaries link; its archives carry the Python packages' copies of the
        # others.
        self.cli_notices = dict(
            project,
            **{
                "LICENSE-THIRD-PARTY.txt": b"Crate: arrow 55.2.0\n",
                "COMPONENTS.tsv": b"name\narrow\t55.2.0\nrust-std\t1.99.1\n",
            },
        )
        for directory, notices in (
            ("python", self.notices),
            ("R/yamaa/inst", self.r_notices),
            (
                "rust/crates/yamaa-cli",
                {
                    name: self.cli_notices[name]
                    for name in ("LICENSE-THIRD-PARTY.txt", "COMPONENTS.tsv")
                },
            ),
        ):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
            for name, payload in notices.items():
                (self.root / directory / name).write_bytes(payload)

    def installed_inventory(self, release):
        """The R inventory that an installation with rustc `release` carries."""
        committed = self.r_notices["COMPONENTS.tsv"].decode("utf-8")
        return inventory.installed_inventory(committed, release).encode("utf-8")

    def tar(self, name, members):
        path = self.root / name
        with tarfile.open(path, "w:gz") as archive:
            for member, payload in members.items():
                entry = tarfile.TarInfo(member)
                entry.size = len(payload)
                archive.addfile(entry, io.BytesIO(payload))
        return path

    def wheel(self, notices, version="0.4.0"):
        path = self.root / f"yamaa-{version}-cp312-abi3-win_amd64.whl"
        with zipfile.ZipFile(path, "w") as archive:
            for name, payload in notices.items():
                archive.writestr(f"yamaa-{version}.dist-info/licenses/{name}", payload)
        return path

    def r_binary(self, name, members):
        """Write an R binary package: a ZIP for Windows, a gzipped tar for macOS."""
        if not name.endswith(".zip"):
            return self.tar(name, members)
        path = self.root / name
        with zipfile.ZipFile(path, "w") as archive:
            for member, payload in members.items():
                archive.writestr(member, payload)
        return path

    def cli(self, notices, version="0.4.0", target="x86_64-unknown-linux-musl"):
        """Write a command-line archive: one directory with the notices."""
        path = self.root / f"yamaa-{version}-{target}.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for name, payload in notices.items():
                archive.writestr(f"yamaa-{version}-{target}/{name}", payload)
        return path

    def test_each_package_kind_carries_the_notices_at_its_own_location(self):
        wheel = self.wheel(self.notices)
        sdist = self.tar(
            "yamaa-0.4.0.tar.gz",
            {f"yamaa-0.4.0/{name}": data for name, data in self.sdist_notices.items()},
        )
        r_source = self.tar(
            "yamaa_0.4.0.tar.gz",
            {f"yamaa/inst/{name}": data for name, data in self.r_notices.items()},
        )
        # An R binary package holds the installed package, where R puts the
        # notices of inst/; its name adds the R series that built it.
        r_binaries = [
            self.r_binary(
                name,
                {
                    f"yamaa/{notice}": data
                    for notice, data in self.r_binary_notices.items()
                },
            )
            for name in ("yamaa_0.4.0-4.6.zip", "yamaa_0.4.0-4.5.tgz")
        ]
        # A command-line archive holds them in its one directory, beside the
        # executable; its name adds the target.
        archives = [
            self.cli(self.cli_notices, target=target)
            for target in ("x86_64-pc-windows-msvc", "aarch64-apple-darwin")
        ]
        for path in (wheel, sdist, r_source, *r_binaries, *archives):
            with self.subTest(archive=path.name):
                self.assertEqual(len(packages.check_archive(path, self.root)), 3)

    def test_command_line_archives_carry_their_own_and_the_shared_notices(self):
        # The command line's own notices and inventory, not a host's; the
        # shared notices are the Python packages' copies.
        member = "yamaa-0.4.0-x86_64-unknown-linux-musl/{}"
        for notice in ("LICENSE-THIRD-PARTY.txt", "COMPONENTS.tsv"):
            for other in (self.notices, self.r_notices):
                archive = self.cli(dict(self.cli_notices, **{notice: other[notice]}))
                with (
                    self.subTest(notice=notice),
                    self.assertRaisesRegex(
                        ValueError, f"differs or is missing: {member.format(notice)}"
                    ),
                ):
                    packages.check_archive(archive, self.root)
        archive = self.cli(dict(self.cli_notices, **{"LICENSE.txt": b"edited\n"}))
        with self.assertRaisesRegex(
            ValueError, f"differs or is missing: {member.format('LICENSE.txt')}"
        ):
            packages.check_archive(archive, self.root)
        (self.root / "python/LICENSE.txt").write_bytes(b"edited\n")
        self.assertEqual(len(packages.check_archive(archive, self.root)), 3)
        # Notices outside the archive's directory are not its notices.
        path = self.root / "yamaa-0.4.0-x86_64-unknown-linux-musl.zip"
        with zipfile.ZipFile(path, "w") as flat:
            for name, payload in self.cli_notices.items():
                flat.writestr(name, payload)
        with self.assertRaisesRegex(ValueError, "differs or is missing"):
            packages.check_archive(path, self.root)

    def test_each_package_carries_its_own_hosts_notices_and_inventory(self):
        # An R source archive with a Python notice, or a wheel with an R one,
        # names the other host's crates.
        for notice in ("LICENSE-THIRD-PARTY.txt", "COMPONENTS.tsv"):
            with self.subTest(notice=notice):
                r_source = self.tar(
                    "yamaa_0.4.0.tar.gz",
                    {
                        f"yamaa/inst/{name}": data
                        for name, data in dict(
                            self.r_notices, **{notice: self.notices[notice]}
                        ).items()
                    },
                )
                with self.assertRaisesRegex(
                    ValueError, f"differs or is missing: yamaa/inst/{notice}"
                ):
                    packages.check_archive(r_source, self.root)
                wheel = self.wheel(
                    dict(self.notices, **{notice: self.r_notices[notice]})
                )
                with self.assertRaisesRegex(
                    ValueError,
                    f"differs or is missing: yamaa-0.4.0.dist-info/licenses/{notice}",
                ):
                    packages.check_archive(wheel, self.root)

    def test_only_the_source_archive_leaves_out_the_standard_library(self):
        # A source archive with the wheels' inventory names a standard library
        # that a build from it need not link; a wheel needs the row.
        sdist = self.tar(
            "yamaa-0.4.0.tar.gz",
            {f"yamaa-0.4.0/{name}": data for name, data in self.notices.items()},
        )
        with self.assertRaisesRegex(
            ValueError, "differs or is missing: yamaa-0.4.0/COMPONENTS.tsv"
        ):
            packages.check_archive(sdist, self.root)
        wheel = self.wheel(self.sdist_notices)
        with self.assertRaisesRegex(
            ValueError,
            "differs or is missing: yamaa-0.4.0.dist-info/licenses/COMPONENTS.tsv",
        ):
            packages.check_archive(wheel, self.root)
        # A wheels' inventory without the row gives no source archive's.
        (self.root / "python/COMPONENTS.tsv").write_bytes(b"name\npyo3\t0.27.2\n")
        sdist = self.tar(
            "yamaa-0.4.0.tar.gz",
            {f"yamaa-0.4.0/{name}": data for name, data in self.sdist_notices.items()},
        )
        with self.assertRaisesRegex(ValueError, "must hold one rust-std row"):
            packages.check_archive(sdist, self.root)

    def test_only_an_r_binary_package_adds_the_standard_library_row(self):
        # configure appends exactly one row, for the rustc that builds the
        # package, to the inventory that R installs; the release binaries are
        # built with the pinned toolchain. The source package carries the
        # committed inventory, which names no standard library.
        committed = self.r_notices["COMPONENTS.tsv"]
        row = self.installed_inventory("1.99.1")[len(committed) :]
        self.assertEqual(row.count(b"\n"), 1)
        self.assertTrue(row.startswith(b"rust-std\t1.99.1\tMIT\t"))
        for case, payload in (
            ("no row", committed),
            ("two rows", committed + row + row),
            ("another release", self.installed_inventory("1.98.0")),
            ("the row first", committed.replace(b"\n", b"\n" + row, 1)),
        ):
            binary = self.r_binary(
                "yamaa_0.4.0-4.6.zip",
                {
                    f"yamaa/{name}": data
                    for name, data in dict(
                        self.r_notices, **{"COMPONENTS.tsv": payload}
                    ).items()
                },
            )
            with (
                self.subTest(case=case),
                self.assertRaisesRegex(
                    ValueError, "differs or is missing: yamaa/COMPONENTS.tsv"
                ),
            ):
                packages.check_archive(binary, self.root)
        r_source = self.tar(
            "yamaa_0.4.0.tar.gz",
            {
                f"yamaa/inst/{name}": data
                for name, data in self.r_binary_notices.items()
            },
        )
        with self.assertRaisesRegex(
            ValueError, "differs or is missing: yamaa/inst/COMPONENTS.tsv"
        ):
            packages.check_archive(r_source, self.root)
        # A committed R inventory that already names a standard library, or a
        # pinned toolchain that is no release, gives no binary's inventory.
        binary = self.r_binary(
            "yamaa_0.4.0-4.5.tgz",
            {f"yamaa/{name}": data for name, data in self.r_binary_notices.items()},
        )
        for path, payload, error in (
            (
                "R/yamaa/inst/COMPONENTS.tsv",
                self.r_binary_notices["COMPONENTS.tsv"],
                "the R source package's inventory must hold no rust-std row",
            ),
            (
                "rust/rust-toolchain.toml",
                b'[toolchain]\nchannel = "stable"\n',
                "pins 'stable', not a Rust release",
            ),
        ):
            original = (self.root / path).read_bytes()
            (self.root / path).write_bytes(payload)
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, error):
                packages.check_archive(binary, self.root)
            (self.root / path).write_bytes(original)
        self.assertEqual(len(packages.check_archive(binary, self.root)), 3)

    def test_misplaced_changed_or_foreign_packages_fail(self):
        misplaced = self.tar(
            "yamaa_0.4.0.tar.gz",
            {f"yamaa/{name}": payload for name, payload in self.r_notices.items()},
        )
        with self.assertRaisesRegex(ValueError, "missing: yamaa/inst/LICENSE.txt"):
            packages.check_archive(misplaced, self.root)
        # A binary package keeps no inst/ directory, and carries the R host's
        # notices.
        for members, error in (
            (
                {f"yamaa/inst/{name}": data for name, data in self.r_notices.items()},
                "missing: yamaa/LICENSE.txt",
            ),
            (
                {f"yamaa/{name}": data for name, data in self.notices.items()},
                "differs or is missing: yamaa/LICENSE-THIRD-PARTY.txt",
            ),
        ):
            binary = self.r_binary("yamaa_0.4.0-4.6.zip", members)
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                packages.check_archive(binary, self.root)
        changed = self.tar(
            "yamaa-0.4.0.tar.gz",
            {
                **{
                    f"yamaa-0.4.0/{name}": data
                    for name, data in self.sdist_notices.items()
                },
                "yamaa-0.4.0/LICENSE-THIRD-PARTY.txt": b"edited\n",
            },
        )
        with self.assertRaisesRegex(ValueError, "LICENSE-THIRD-PARTY.txt"):
            packages.check_archive(changed, self.root)
        other = self.tar("yamaa-0.4.1.tar.gz", {})
        with self.assertRaisesRegex(ValueError, "not a yamaa 0.4.0"):
            packages.check_archive(other, self.root)

    def vendor_archive(self, members):
        """The xz-compressed tar archive of vendored crates in an R package."""
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:xz") as archive:
            for member, payload in members.items():
                entry = tarfile.TarInfo(member)
                entry.size = len(payload)
                archive.addfile(entry, io.BytesIO(payload))
        return data.getvalue()

    def test_no_text_names_a_gnu_license_outside_the_reviewed_vendored_crates(self):
        gpl = b"GNU GENERAL PUBLIC LICENSE\nVersion 2, June 1991\n"
        llvm = (
            b"In addition, if you combine or link compiled forms of this Software "
            b'with\nsoftware that is licensed under the GPLv2 ("Combined Software") '
            b"and if a\ncourt of competent jurisdiction determines that the patent "
            b"provision (Section\n3), the indemnity provision (Section 9) or other "
            b"Section of the License\nconflicts with the conditions of the GPLv2, "
            b"you may retroactively and\nprospectively choose to deem waived or "
            b"otherwise exclude such Section(s) of\nthe License, but only in their "
            b"entirety and only with respect to the Combined\nSoftware.\n"
        )
        vendored = {
            # zstd's own sources and r-efi's notice, as cargo vendor writes them.
            "vendor/zstd-sys-2.1.2+zstd.1.5.7/zstd/COPYING": gpl,
            "vendor/r-efi-5.3.0/AUTHORS": b"the GNU Lesser General Public License\n",
            # The LLVM exception names GPLv2 but grants no GNU license.
            "vendor/rustix-1.1.5/LICENSE-Apache-2.0_WITH_LLVM-exception": llvm,
            "vendor/windows-sys-0.61.2/src/lib.rs": b"pub lgpl: LOGPALETTE,\n",
        }
        members = {f"yamaa/inst/{name}": data for name, data in self.r_notices.items()}
        members["yamaa/src/rust/vendor.tar.xz"] = self.vendor_archive(vendored)
        # Compiled code holds no license text to read.
        members["yamaa/src/yamaa.so"] = b"\xff\xfeGPLv3\x00"
        r_source = self.tar("yamaa_0.4.0.tar.gz", members)
        self.assertEqual(len(packages.check_archive(r_source, self.root)), 3)
        # Any other crate, and any text of yamaa's own, must name none.
        members["yamaa/src/rust/vendor.tar.xz"] = self.vendor_archive(
            dict(vendored, **{"vendor/other-1.0.0/README.md": b"GPL-3.0-only\n"})
        )
        members["yamaa/src/rust/vendor/saphyr-parser/README.md"] = b"See the LGPL.\n"
        r_source = self.tar("yamaa_0.4.0.tar.gz", members)
        with self.assertRaises(ValueError) as raised:
            packages.check_archive(r_source, self.root)
        self.assertEqual(
            str(raised.exception).splitlines(),
            [
                "yamaa_0.4.0.tar.gz: yamaa/src/rust/vendor.tar.xz!vendor/"
                "other-1.0.0/README.md names a GNU license (GPL)",
                "yamaa_0.4.0.tar.gz: yamaa/src/rust/vendor/saphyr-parser/README.md "
                "names a GNU license (LGPL)",
            ],
        )
        # The Python source archive carries its vendored crates unpacked.
        sdist = self.tar(
            "yamaa-0.4.0.tar.gz",
            {
                **{
                    f"yamaa-0.4.0/{name}": data
                    for name, data in self.sdist_notices.items()
                },
                "yamaa-0.4.0/vendor/zstd-sys-2.1.2+zstd.1.5.7/zstd/COPYING": gpl,
            },
        )
        self.assertEqual(len(packages.check_archive(sdist, self.root)), 3)
        wheel = self.wheel(
            dict(self.notices, **{"LICENSE-THIRD-PARTY.txt": b"Crate: r-efi\nLGPL\n"})
        )
        (self.root / "python/LICENSE-THIRD-PARTY.txt").write_bytes(
            b"Crate: r-efi\nLGPL\n"
        )
        with self.assertRaisesRegex(
            ValueError,
            "yamaa-0.4.0.dist-info/licenses/LICENSE-THIRD-PARTY.txt names a GNU "
            r"license \(LGPL\)",
        ):
            packages.check_archive(wheel, self.root)

        # Command-line archives also reject a forbidden license even when
        # their notice agrees with the checkout's own copy.
        forbidden = b"Crate: r-efi\nLGPL\n"
        cli = self.cli(dict(self.cli_notices, **{"LICENSE-THIRD-PARTY.txt": forbidden}))
        (self.root / "rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt").write_bytes(
            forbidden
        )
        with self.assertRaisesRegex(
            ValueError,
            r"LICENSE-THIRD-PARTY.txt names a GNU license \(LGPL\)",
        ):
            packages.check_archive(cli, self.root)

    def test_a_candidate_names_python_packages_and_the_r_archive_differently(self):
        # #3030 D53: the R package keeps the final version through candidates.
        (self.root / "tools/packaging/release.toml").write_text(
            'version = "1.0.0"\ncandidate = 1\n'
        )
        wheel = self.wheel(self.notices, version="1.0.0rc1")
        sdist = self.tar(
            "yamaa-1.0.0rc1.tar.gz",
            {
                f"yamaa-1.0.0rc1/{name}": data
                for name, data in self.sdist_notices.items()
            },
        )
        r_source = self.tar(
            "yamaa_1.0.0.tar.gz",
            {f"yamaa/inst/{name}": data for name, data in self.r_notices.items()},
        )
        r_binary = self.r_binary(
            "yamaa_1.0.0-4.6.tgz",
            {f"yamaa/{name}": data for name, data in self.r_binary_notices.items()},
        )
        # The command line's archives carry the engine's Cargo version.
        cli = self.cli(self.cli_notices, version="1.0.0-rc.1")
        for path in (wheel, sdist, r_source, r_binary, cli):
            with self.subTest(archive=path.name):
                self.assertEqual(len(packages.check_archive(path, self.root)), 3)
        for name in (
            "yamaa_1.0.0rc1.tar.gz",
            "yamaa-1.0.0.tar.gz",
            "yamaa_1.0.0rc1-4.6.tgz",
            "yamaa_1.0.0.zip",
            "yamaa_1.0.0-4.6.tar.gz",
            "yamaa-1.0.0rc1-x86_64-unknown-linux-musl.zip",
            "yamaa-1.0.0-x86_64-unknown-linux-musl.zip",
            "yamaa-1.0.0-rc.1-x86_64-unknown-linux-gnu.zip",
        ):
            other = self.tar(name, {})
            with (
                self.subTest(archive=name),
                self.assertRaisesRegex(
                    ValueError, "not a yamaa 1.0.0 release candidate 1 wheel"
                ),
            ):
                packages.check_archive(other, self.root)


if __name__ == "__main__":
    unittest.main()
