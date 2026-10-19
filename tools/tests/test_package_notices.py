"""Each built package carries every package notice at its own location."""

import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from tools.packaging import check_package_notices as packages


class PackageNotices(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "python").mkdir()
        (self.root / "tools/packaging").mkdir(parents=True)
        (self.root / "tools/packaging/release.toml").write_text('version = "0.4.0"\n')
        (self.root / "python/pyproject.toml").write_text(
            '[project]\nversion = "0.4.0"\n'
            'license-files = ["LICENSE.txt", "LICENSE-THIRD-PARTY.txt"]\n'
        )
        self.notices = {
            "LICENSE.txt": b"project\n",
            "LICENSE-THIRD-PARTY.txt": b"crates\n",
        }
        for name, payload in self.notices.items():
            (self.root / "python" / name).write_bytes(payload)

    def tar(self, name, members):
        path = self.root / name
        with tarfile.open(path, "w:gz") as archive:
            for member, payload in members.items():
                entry = tarfile.TarInfo(member)
                entry.size = len(payload)
                archive.addfile(entry, io.BytesIO(payload))
        return path

    def test_each_package_kind_carries_the_notices_at_its_own_location(self):
        wheel = self.root / "yamaa-0.4.0-cp312-abi3-win_amd64.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            for name, payload in self.notices.items():
                archive.writestr(f"yamaa-0.4.0.dist-info/licenses/{name}", payload)
        sdist = self.tar(
            "yamaa-0.4.0.tar.gz",
            {f"yamaa-0.4.0/{name}": payload for name, payload in self.notices.items()},
        )
        r_source = self.tar(
            "yamaa_0.4.0.tar.gz",
            {f"yamaa/inst/{name}": payload for name, payload in self.notices.items()},
        )
        for path in (wheel, sdist, r_source):
            with self.subTest(archive=path.name):
                self.assertEqual(len(packages.check_archive(path, self.root)), 2)

    def test_misplaced_changed_or_foreign_packages_fail(self):
        misplaced = self.tar(
            "yamaa_0.4.0.tar.gz",
            {f"yamaa/{name}": payload for name, payload in self.notices.items()},
        )
        with self.assertRaisesRegex(ValueError, "missing: yamaa/inst/LICENSE.txt"):
            packages.check_archive(misplaced, self.root)
        changed = self.tar(
            "yamaa-0.4.0.tar.gz",
            {
                **{f"yamaa-0.4.0/{name}": data for name, data in self.notices.items()},
                "yamaa-0.4.0/LICENSE-THIRD-PARTY.txt": b"edited\n",
            },
        )
        with self.assertRaisesRegex(ValueError, "LICENSE-THIRD-PARTY.txt"):
            packages.check_archive(changed, self.root)
        other = self.tar("yamaa-0.4.1.tar.gz", {})
        with self.assertRaisesRegex(ValueError, "not a yamaa 0.4.0"):
            packages.check_archive(other, self.root)

    def test_a_candidate_names_python_packages_and_the_r_archive_differently(self):
        # #3030 D53: the R package keeps the final version through candidates.
        (self.root / "tools/packaging/release.toml").write_text(
            'version = "1.0.0"\ncandidate = 1\n'
        )
        wheel = self.root / "yamaa-1.0.0rc1-cp312-abi3-win_amd64.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            for name, payload in self.notices.items():
                archive.writestr(f"yamaa-1.0.0rc1.dist-info/licenses/{name}", payload)
        sdist = self.tar(
            "yamaa-1.0.0rc1.tar.gz",
            {f"yamaa-1.0.0rc1/{name}": data for name, data in self.notices.items()},
        )
        r_source = self.tar(
            "yamaa_1.0.0.tar.gz",
            {f"yamaa/inst/{name}": data for name, data in self.notices.items()},
        )
        for path in (wheel, sdist, r_source):
            with self.subTest(archive=path.name):
                self.assertEqual(len(packages.check_archive(path, self.root)), 2)
        for name in ("yamaa_1.0.0rc1.tar.gz", "yamaa-1.0.0.tar.gz"):
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
