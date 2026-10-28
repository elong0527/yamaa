"""Release only complete native qualification and its unchanged tested packages."""

import contextlib
import copy
import functools
import io
import os
import re
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

from tools.ci import publish_release as publisher
from tools.ci import release_assets, toolchain
from tools.ci import select_release_artifacts as release
from tools.packaging import cli_archive, conformance_archive, release_identity
from tools.packaging import generate_component_inventory as inventory
from tools.packaging.release_identity import Identity
from tools.paths import ROOT
from tools.tests import support

MATRIX = release.native
PROJECT = tomllib.loads((ROOT / "python/pyproject.toml").read_text(encoding="utf-8"))[
    "project"
]
# The release the tree prepares, and its Python and R spellings.
IDENTITY = release_identity.read(ROOT)
VERSION = IDENTITY.python
R_VERSION = IDENTITY.r
# Every package carries the project license and these notices unchanged.
NOTICES = PROJECT["license-files"]
TAG = IDENTITY.tag
major, minor, patch_level = IDENTITY.version.split(".")
OTHER_VERSION = f"{major}.{minor}.{int(patch_level) + 1}"
ASSETS = release_assets.declared(MATRIX.coverage())
REVISION = "a" * 40
REPOSITORY = {
    "id": 1299533902,
    "full_name": "elong0527/yamaa",
    "default_branch": "main",
}


def native_run():
    return {
        "id": 7,
        "run_attempt": 2,
        "head_sha": REVISION,
        "status": "completed",
        "conclusion": "success",
        "event": "schedule",
        "head_branch": "main",
        "path": ".github/workflows/native.yml",
        "html_url": "https://github.com/elong0527/yamaa/actions/runs/7",
        "repository": dict(REPOSITORY),
        "head_repository": dict(REPOSITORY),
    }


# Each attempt's job start, artifact upload and job completion. The run was
# re-run an hour after attempt 1 completed; attempt 2 qualifies.
ATTEMPTS = {
    1: ("2026-01-01T01:00:00Z", "2026-01-01T01:15:00Z", "2026-01-01T01:30:00Z"),
    2: ("2026-01-01T02:00:00Z", "2026-01-01T02:15:00Z", "2026-01-01T02:30:00Z"),
}


def native_jobs(carried_over=()):
    """List the qualifying attempt's jobs as its jobs endpoint does.

    The jobs and critical steps are the selector's own; the native.yml
    contract test checks them against the workflow. "Re-run failed jobs"
    lists a job it did not run again with attempt 1's execution times.
    """
    rows = []
    for name, steps in release.required_jobs(MATRIX.coverage()).items():
        started, _, completed = ATTEMPTS[1 if name in carried_over else 2]
        rows.append(
            {
                "name": name,
                "status": "completed",
                "conclusion": "success",
                "head_sha": REVISION,
                "run_attempt": 2,
                "started_at": started,
                "completed_at": completed,
                "steps": [
                    {"name": step, "status": "completed", "conclusion": "success"}
                    for step in steps
                ],
            }
        )
    return rows


def native_artifacts(attempt=2):
    """List the release artifacts one attempt's producing jobs uploaded."""
    producers = release_assets.artifacts(ASSETS)
    return [
        {
            # Artifact IDs are unique across a run's attempts.
            "id": index + (10 if attempt == 2 else 20),
            "name": name,
            "expired": False,
            "created_at": ATTEMPTS[attempt][1],
            "workflow_run": {
                "id": 7,
                "head_sha": REVISION,
                "repository_id": REPOSITORY["id"],
                "head_repository_id": REPOSITORY["id"],
            },
        }
        for index, name in enumerate(producers)
    ]


def tagged_repository(root, messages):
    """Commit once on main and tag that commit under each name.

    A message makes an annotated tag; None makes a lightweight one.
    """
    support.init(root)
    support.git(root, "commit", "--allow-empty", "-m", "qualified")
    support.git(root, "update-ref", "refs/remotes/origin/main", "HEAD")
    for name, message in messages.items():
        if message is None:
            support.git(root, "tag", name)
        else:
            support.git(root, "tag", "-a", name, "-m", message)


def tar(path, members):
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in members.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(payload)
            archive.addfile(entry, io.BytesIO(payload))


def example_name(asset, identity):
    """A file name the asset can have, with an example platform version."""
    return (
        asset.file.replace("<Python version>", identity.python)
        .replace("<R version>", identity.r)
        .replace("<Cargo version>", identity.cargo)
        .replace("*", "2_39")
    )


def r_binary_description(asset, version):
    """The DESCRIPTION that R CMD INSTALL --build writes into a binary package.

    The Built field names the R and the platform of the build: macOS adds the
    Darwin version to the platform that the asset declares, and R for
    Windows writes the file with CRLF line ends.
    """
    if asset.file.endswith(".zip"):
        built, newline = f"{asset.built}; 2026-01-01 00:00:00 UTC; windows", "\r\n"
    else:
        built, newline = f"{asset.built}20; 2026-01-01 00:00:00 UTC; unix", "\n"
    fields = ["Package: yamaa", f"Version: {version}", f"Built: {built}"]
    return "".join(field + newline for field in fields).encode()


def r_binary(path, members):
    """Write an R binary package: a ZIP on Windows, a gzipped tar on macOS."""
    if path.suffix == ".zip":
        with zipfile.ZipFile(path, "w") as archive:
            for name, payload in members.items():
                archive.writestr(name, payload)
    else:
        tar(path, members)


def executable(target, marker=b""):
    """An executable fixture whose header names the format and architecture
    of the target's, followed by bytes that no other fixture has."""
    kind, architecture = cli_archive.expected_format(target)
    machine = {name: code for code, name in cli_archive.MACHINES[kind].items()}[
        architecture
    ]
    if kind == "ELF":
        header = b"\x7fELF\x02\x01" + bytes(12) + struct.pack("<H", machine)
    elif kind == "Mach-O":
        header = b"\xcf\xfa\xed\xfe" + struct.pack("<I", machine)
    else:
        header = b"MZ" + bytes(58) + struct.pack("<I", 64) + b"PE\0\0"
        header += struct.pack("<H", machine)
    return header + b"independent native fixture\0" + marker


def cli_zip(path, members):
    """Write a command-line archive whose members map to (payload, mode)."""
    with zipfile.ZipFile(path, "w") as archive:
        for name, (payload, mode) in members.items():
            cli_archive.add(archive, name, payload, mode, (2026, 1, 1, 0, 0, 0))


def cli_members(identity, target, notices, marker=b""):
    """The members of a target's command-line archive, with their modes."""
    program, members = cli_archive.layout(identity, target)
    return {
        program: (executable(target, marker), cli_archive.EXECUTABLE_MODE),
        **{
            member: (notices[notice], cli_archive.NOTICE_MODE)
            for notice, member in members.items()
        },
    }


def downloads(root, identity=IDENTITY):
    """Write each release artifact as the native run uploads it."""
    licenses = {name: (ROOT / "python" / name).read_bytes() for name in NOTICES}
    # The Python source archive's inventory leaves out the standard library
    # that the wheels link.
    wheels = licenses[inventory.NAME].decode("utf-8")
    sdist_licenses = dict(
        licenses, **{inventory.NAME: inventory.source_inventory(wheels).encode()}
    )
    # The R package's notices are the same, apart from its third-party notices
    # and component inventory, which list its own crates.
    r_licenses = {name: (ROOT / "R/yamaa/inst" / name).read_bytes() for name in NOTICES}
    # An R binary package's inventory also names the standard library of the
    # pinned toolchain that builds it, which configure appends when R
    # installs the package.
    r_inventory = r_licenses[inventory.NAME].decode("utf-8")
    r_binary_licenses = dict(
        r_licenses,
        **{
            inventory.NAME: inventory.installed_inventory(
                r_inventory, toolchain.channel()
            ).encode()
        },
    )
    # A command-line archive carries the command line's own notices and
    # inventory, and the Python packages' copies of the shared notices; one
    # for a musl target also carries the command line's notice of musl.
    cli_licenses = dict(
        licenses,
        **{
            name: (ROOT / "rust/crates/yamaa-cli" / name).read_bytes()
            for name in ("LICENSE-THIRD-PARTY.txt", inventory.NAME, "LICENSE-MUSL.txt")
        },
    )
    python, r = identity.python, identity.r
    metadata = f"Metadata-Version: 2.4\nName: yamaa\nVersion: {python}\n\n".encode()
    for asset in release_assets.declared(MATRIX.coverage()):
        directory = root / asset.artifact
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / example_name(asset, identity)
        if asset.kind == "wheel":
            with zipfile.ZipFile(path, "w") as wheel:
                wheel.writestr(f"yamaa-{python}.dist-info/METADATA", metadata)
                wheel.writestr(
                    "yamaa/_native.so",
                    b"independent native fixture\0" + asset.artifact.encode(),
                )
                for name, payload in licenses.items():
                    wheel.writestr(f"yamaa-{python}.dist-info/licenses/{name}", payload)
        elif asset.kind == "sdist":
            tar(
                path,
                {
                    f"yamaa-{python}/PKG-INFO": metadata,
                    f"yamaa-{python}/platform.txt": asset.artifact.encode(),
                    # Vendored sources keep their own, differently worded notices.
                    f"yamaa-{python}/rust/vendor/saphyr-parser/LICENSE": b"vendored\n",
                    **{
                        f"yamaa-{python}/{name}": payload
                        for name, payload in sdist_licenses.items()
                    },
                },
            )
        elif asset.kind == "cli":
            target = asset.artifact.removeprefix("native-cli-")
            cli_zip(
                path,
                cli_members(identity, target, cli_licenses, asset.artifact.encode()),
            )
        elif asset.kind == "conformance":
            # The committed files, which the selector compares with the
            # checkout, under the directory that the identity names.
            cli_zip(
                path,
                {
                    member: ((ROOT / file).read_bytes(), conformance_archive.MODE)
                    for member, file in conformance_archive.layout(identity).items()
                },
            )
        elif asset.kind == "r-binary":
            # R installs the notices of inst/ at the package root.
            r_binary(
                path,
                {
                    "yamaa/DESCRIPTION": r_binary_description(asset, r),
                    "yamaa/LICENSE": (
                        b"YEAR: 2026\nCOPYRIGHT HOLDER: yamaa contributors\n"
                    ),
                    "yamaa/libs/yamaa": b"independent native fixture\0"
                    + asset.artifact.encode(),
                    **{
                        f"yamaa/{name}": payload
                        for name, payload in r_binary_licenses.items()
                    },
                },
            )
        else:
            tar(
                path,
                {
                    "yamaa/DESCRIPTION": f"Package: yamaa\nVersion: {r}\n".encode(),
                    # R's MIT license stub, which the full text in inst/ accompanies.
                    "yamaa/LICENSE": (
                        b"YEAR: 2026\nCOPYRIGHT HOLDER: yamaa contributors\n"
                    ),
                    **{
                        f"yamaa/inst/{name}": payload
                        for name, payload in r_licenses.items()
                    },
                },
            )


class QualificationSelection(unittest.TestCase):
    def setUp(self):
        self.run = native_run()
        self.coverage = MATRIX.coverage()
        self.jobs = release.validate_jobs(native_jobs(), self.coverage, self.run)

    def select(self, artifacts, jobs=None):
        return release.select_artifacts(
            artifacts, self.coverage, self.run, REPOSITORY, jobs or self.jobs
        )

    def test_main_schedule_dispatch_and_same_repository_qualification_push_are_complete(
        self,
    ):
        for event, branch in [
            ("schedule", "main"),
            ("workflow_dispatch", "main"),
            ("push", "qualification/issue-2456-final"),
        ]:
            with self.subTest(event=event, branch=branch):
                run = dict(self.run, event=event, head_branch=branch)
                self.assertEqual(
                    release.run_coverage(run, REPOSITORY, REVISION), self.coverage
                )

    def test_wrong_run_source_authority_workflow_event_or_attempt_cannot_release(self):
        mutations = [
            ("head_sha", "b" * 40),
            ("status", "in_progress"),
            ("conclusion", "failure"),
            ("path", ".github/workflows/ci.yml"),
            ("path", None),
            ("run_attempt", 0),
            ("id", True),
            ("repository", dict(REPOSITORY, id=1)),
            ("head_repository", dict(REPOSITORY, full_name="fork/yamaa")),
            ("html_url", "https://github.com/fork/yamaa/actions/runs/7"),
            ("event", "pull_request"),
            ("head_branch", "feature/installed-only"),
            ("event", "push"),
        ]
        for key, value in mutations:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                release.run_coverage(
                    dict(self.run, **{key: value}), REPOSITORY, REVISION
                )
        with self.assertRaisesRegex(ValueError, "main dispatch"):
            release.run_coverage(
                dict(
                    self.run,
                    event="workflow_dispatch",
                    head_branch="qualification/issue-2456",
                ),
                REPOSITORY,
                REVISION,
            )

    def test_every_missing_or_skipped_required_job_and_critical_step_fails(self):
        for index, job in enumerate(native_jobs()):
            with self.subTest(missing=job["name"]), self.assertRaises(ValueError):
                jobs = native_jobs()
                del jobs[index]
                release.validate_jobs(jobs, self.coverage, self.run)
            if job["conclusion"] == "skipped":
                continue
            with self.subTest(skipped=job["name"]), self.assertRaises(ValueError):
                jobs = native_jobs()
                jobs[index]["conclusion"] = "skipped"
                release.validate_jobs(jobs, self.coverage, self.run)
            for step_index, step in enumerate(job["steps"]):
                with (
                    self.subTest(job=job["name"], skipped_step=step["name"]),
                    self.assertRaises(ValueError),
                ):
                    jobs = native_jobs()
                    jobs[index]["steps"][step_index]["conclusion"] = "skipped"
                    release.validate_jobs(jobs, self.coverage, self.run)

    def test_jobs_cannot_mix_attempts_sources_or_duplicate_names(self):
        for key, value in [("head_sha", "b" * 40), ("run_attempt", 1)]:
            jobs = native_jobs()
            jobs[0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                release.validate_jobs(jobs, self.coverage, self.run)
        jobs = native_jobs()
        jobs.append(copy.deepcopy(jobs[0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            release.validate_jobs(jobs, self.coverage, self.run)

    def test_artifacts_require_unique_unexpired_same_run_and_repository_identities(
        self,
    ):
        selected = self.select(native_artifacts())
        self.assertEqual(len(selected), len(release_assets.artifacts(ASSETS)))
        for index in range(len(selected)):
            for defect in (
                "missing",
                "duplicate",
                "expired",
                "source",
                "run",
                "head_repository",
            ):
                artifacts = native_artifacts()
                if defect == "missing":
                    del artifacts[index]
                elif defect == "duplicate":
                    artifacts.append(copy.deepcopy(artifacts[index]))
                elif defect == "expired":
                    artifacts[index]["expired"] = True
                else:
                    field, value = {
                        "source": ("head_sha", "b" * 40),
                        "run": ("id", 8),
                        "head_repository": ("head_repository_id", 1),
                    }[defect]
                    artifacts[index]["workflow_run"][field] = value
                with (
                    self.subTest(index=index, defect=defect),
                    self.assertRaises(ValueError),
                ):
                    self.select(artifacts)

    def test_a_full_rerun_selects_the_artifacts_its_qualifying_attempt_uploaded(self):
        # "Re-run all jobs" keeps attempt 1's artifacts beside attempt 2's.
        earlier, qualifying = native_artifacts(attempt=1), native_artifacts()
        for artifacts in (earlier + qualifying, qualifying + earlier):
            self.assertEqual(self.select(artifacts), qualifying)

    def test_a_failed_job_rerun_keeps_the_artifacts_of_the_producers_it_carried_over(
        self,
    ):
        producers = release_assets.artifacts(ASSETS)
        jobs = release.validate_jobs(
            native_jobs(carried_over=producers.values()), self.coverage, self.run
        )
        earlier = native_artifacts(attempt=1)
        self.assertEqual(self.select(earlier, jobs), earlier)
        # A producer that attempt 2 did not run uploaded nothing during it.
        with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
            self.select(native_artifacts(), jobs)

    def test_an_artifact_counts_only_if_created_while_its_producer_ran(self):
        started, _, completed = ATTEMPTS[2]
        for created in (started, completed):
            artifacts = native_artifacts()
            artifacts[0]["created_at"] = created
            self.assertEqual(self.select(artifacts), artifacts)
        for created, error in [
            ("2026-01-01T01:59:59Z", "missing or ambiguous"),
            ("2026-01-01T02:30:01Z", "missing or ambiguous"),
            (None, "malformed GitHub timestamp"),
            ("2026-01-01 02:15:00", "malformed GitHub timestamp"),
            ("2026-01-01T02:15:00+00:00", "malformed GitHub timestamp"),
            ("2026-01-01T02:15:00.5Z", "malformed GitHub timestamp"),
        ]:
            artifacts = native_artifacts()
            artifacts[0]["created_at"] = created
            with (
                self.subTest(created=created),
                self.assertRaisesRegex(ValueError, error),
            ):
                self.select(artifacts)
        producer = next(iter(release_assets.artifacts(ASSETS).values()))
        for field in ("started_at", "completed_at"):
            jobs = copy.deepcopy(self.jobs)
            del jobs[producer][field]
            with (
                self.subTest(field=field),
                self.assertRaisesRegex(ValueError, "malformed GitHub timestamp"),
            ):
                self.select(native_artifacts(), jobs)

    def test_paginated_jobs_artifacts_and_releases_keep_all_pages(self):
        for field in ("jobs", "artifacts", "workflow_runs", None):
            with self.subTest(field=field):
                values = [[{"id": 1}], [{"id": 2}]]
                pages = [{field: page} for page in values] if field else values
                with patch.object(release, "gh_json", return_value=pages) as gh:
                    self.assertEqual(
                        release.paginated(
                            REPOSITORY["full_name"], "collection?per_page=100", field
                        ),
                        values[0] + values[1],
                    )
                    self.assertEqual(
                        gh.call_args.args[:3], ("api", "--paginate", "--slurp")
                    )
        with (
            patch.object(release, "gh_json", return_value=[{"wrong": []}]),
            self.assertRaises(ValueError),
        ):
            release.paginated(REPOSITORY["full_name"], "jobs", "jobs")


class DownloadedPackages(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        downloads(self.directory)
        self.selected = native_artifacts()

    def select(self):
        return release.downloaded_assets(
            self.directory, self.selected, IDENTITY, ROOT, ASSETS
        )

    def test_every_declared_asset_keeps_its_exact_bytes_in_declaration_order(self):
        before = {
            path: path.read_bytes()
            for path in self.directory.rglob("*")
            if path.is_file()
        }
        assets = self.select()
        # Every file of every release artifact is published.
        self.assertEqual(sorted(assets), sorted(before))
        self.assertEqual(
            [(path.parent.name, path.name) for path in assets],
            [(asset.artifact, example_name(asset, IDENTITY)) for asset in ASSETS],
        )
        self.assertEqual(
            sum(path.suffix == ".whl" for path in assets),
            len(MATRIX.coverage()["build"]["include"]),
        )
        for path in assets:
            self.assertEqual(path.read_bytes(), before[path])
            if path.name.endswith(".tar.gz"):
                self.assertIn(MATRIX.SOURCE_PLATFORM, path.parent.name)
        self.assertEqual(before, {path: path.read_bytes() for path in before})

    def test_a_missing_or_undeclared_file_fails_the_whole_selection(self):
        # Another host's source archive is not an asset: publishing all
        # assets or none leaves no file behind unannounced.
        source = next(self.directory.rglob(f"yamaa-{VERSION}.tar.gz"))
        other = next(
            asset.artifact
            for asset in ASSETS
            if asset.kind == "wheel" and MATRIX.SOURCE_PLATFORM not in asset.artifact
        )
        copy = self.directory / other / source.name
        copy.write_bytes(source.read_bytes())
        with self.assertRaisesRegex(ValueError, "no release asset declares"):
            self.select()
        copy.unlink()
        moved = source.with_name("moved")
        source.rename(moved)
        with self.assertRaisesRegex(ValueError, "Python source archive"):
            self.select()
        moved.rename(source)
        self.assertEqual(len(self.select()), len(ASSETS))

    def test_filename_version_host_abi_and_extra_package_ambiguity_fail(self):
        wheel = next(
            (self.directory / f"native-python-{MATRIX.SOURCE_PLATFORM}-3.14").glob(
                "*.whl"
            )
        )
        platform_tag = wheel.name.removesuffix(".whl").rsplit("-", 1)[1]
        for bad in [
            wheel.name.replace(VERSION, OTHER_VERSION, 1),
            wheel.name.replace("abi3", "cp312"),
            wheel.name.replace(platform_tag, "win_amd64"),
            wheel.name.replace(platform_tag, "linux_x86_64"),
            wheel.name.replace(platform_tag, platform_tag.replace("2_28", "2_34")),
        ]:
            replacement = wheel.with_name(bad)
            wheel.rename(replacement)
            try:
                with self.subTest(filename=bad), self.assertRaises(ValueError):
                    self.select()
            finally:
                replacement.rename(wheel)
        extra = wheel.with_name("duplicate.whl")
        extra.write_bytes(wheel.read_bytes())
        with self.assertRaisesRegex(ValueError, "no release asset declares"):
            self.select()
        extra.unlink()
        # Two files that one declared name pattern admits.
        macos = next((self.directory / "native-python-macos-15-3.14").glob("*.whl"))
        extra = macos.with_name(macos.name.replace("2_39", "2_40"))
        extra.write_bytes(macos.read_bytes())
        with self.assertRaisesRegex(ValueError, "missing or ambiguous release asset"):
            self.select()

    def test_metadata_version_duplicates_missing_licenses_and_r_description_fail(self):
        wheel = next(self.directory.rglob("*.whl"))
        original = wheel.read_bytes()
        version_header = f"Version: {VERSION}".encode("ascii")
        wrong_version_header = f"Version: {OTHER_VERSION}".encode("ascii")
        for defect in (
            "version",
            "duplicate-version",
            "missing-license",
            "changed-license",
        ):
            with (
                zipfile.ZipFile(io.BytesIO(original)) as source,
                zipfile.ZipFile(wheel, "w") as target,
            ):
                for entry in source.infolist():
                    payload = source.read(entry.filename)
                    if defect == "missing-license" and entry.filename.endswith(
                        "LICENSE-THIRD-PARTY.txt"
                    ):
                        continue
                    if entry.filename.endswith("/METADATA"):
                        if defect == "version":
                            payload = payload.replace(
                                version_header, wrong_version_header
                            )
                        elif defect == "duplicate-version":
                            payload = payload.replace(
                                version_header, version_header + b"\n" + version_header
                            )
                    if defect == "changed-license" and entry.filename.endswith(
                        "LICENSE-THIRD-PARTY.txt"
                    ):
                        payload = b"not the required notices"
                    target.writestr(entry, payload)
            with self.subTest(defect=defect), self.assertRaises(ValueError):
                self.select()
            wheel.write_bytes(original)
        r_source = next(self.directory.rglob(f"yamaa_{R_VERSION}.tar.gz"))
        with tarfile.open(r_source) as source:
            members = {
                entry.name: source.extractfile(entry).read()
                for entry in source
                if entry.isfile()
            }
        members["yamaa/DESCRIPTION"] = (
            f"Package: yamaa\nVersion: {OTHER_VERSION}\n".encode("ascii")
        )
        tar(r_source, members)
        with self.assertRaisesRegex(ValueError, "R DESCRIPTION"):
            self.select()

    def test_source_archives_carry_each_notice_at_its_package_location(self):
        """A copy elsewhere in an archive does not replace the package notice."""
        sdist = next(self.directory.rglob(f"yamaa-{VERSION}.tar.gz"))
        r_source = next(self.directory.rglob(f"yamaa_{R_VERSION}.tar.gz"))
        stub = b"YEAR: 2026\nCOPYRIGHT HOLDER: yamaa contributors\n"
        for path, name, moved, changed in (
            (
                sdist,
                f"yamaa-{VERSION}/LICENSE-THIRD-PARTY.txt",
                f"yamaa-{VERSION}/python/LICENSE-THIRD-PARTY.txt",
                None,
            ),
            (sdist, f"yamaa-{VERSION}/LICENSE.txt", None, None),
            (r_source, "yamaa/inst/LICENSE-THIRD-PARTY.txt", None, None),
            (r_source, "yamaa/inst/LICENSE.txt", "yamaa/LICENSE.txt", None),
            (r_source, "yamaa/inst/LICENSE.txt", None, stub),
        ):
            original = path.read_bytes()
            with tarfile.open(path) as source:
                members = {
                    entry.name: source.extractfile(entry).read()
                    for entry in source
                    if entry.isfile()
                }
            payload = members.pop(name)
            if moved is not None:
                members[moved] = payload
            if changed is not None:
                members[name] = changed
            tar(path, members)
            try:
                with (
                    self.subTest(archive=path.name, member=name, moved=moved),
                    self.assertRaisesRegex(ValueError, "required notice"),
                ):
                    self.select()
            finally:
                path.write_bytes(original)
        self.assertEqual(len(self.select()), len(ASSETS))

    def test_r_binaries_carry_their_version_build_and_notices(self):
        """Each R binary package names its release, its R and its platform."""
        assets = [asset for asset in ASSETS if asset.kind == "r-binary"]
        self.assertTrue(assets)

        def r_of(asset):
            return asset.built.split(";")[0]

        for asset in assets:
            path = self.directory / asset.artifact / example_name(asset, IDENTITY)
            original = path.read_bytes()
            members = self.read(path)
            description = members["yamaa/DESCRIPTION"]
            extension = path.suffix
            other_r = next(
                other
                for other in assets
                if other.file.endswith(extension) and r_of(other) != r_of(asset)
            )
            other_platform = next(
                other
                for other in assets
                if not other.file.endswith(extension) and r_of(other) == r_of(asset)
            )
            moved = {
                name.replace("yamaa/", "yamaa/inst/", 1)
                if name.startswith("yamaa/LICENSE-")
                else name: payload
                for name, payload in members.items()
            }
            for defect, replaced, error in (
                (
                    "another version",
                    dict(
                        members,
                        **{
                            "yamaa/DESCRIPTION": description.replace(
                                f"Version: {R_VERSION}".encode(),
                                f"Version: {OTHER_VERSION}".encode(),
                            )
                        },
                    ),
                    "R DESCRIPTION",
                ),
                (
                    "no build",
                    dict(
                        members,
                        **{"yamaa/DESCRIPTION": description.split(b"Built:")[0]},
                    ),
                    "not built with",
                ),
                (
                    "another R series",
                    dict(
                        members,
                        **{
                            "yamaa/DESCRIPTION": r_binary_description(
                                other_r, R_VERSION
                            )
                        },
                    ),
                    "not built with",
                ),
                (
                    "another platform",
                    dict(
                        members,
                        **{
                            "yamaa/DESCRIPTION": r_binary_description(
                                other_platform, R_VERSION
                            )
                        },
                    ),
                    "not built with",
                ),
                ("notices left in inst", moved, "required notice"),
                (
                    "the source package's inventory",
                    dict(
                        members,
                        **{
                            f"yamaa/{inventory.NAME}": (
                                ROOT / "R/yamaa/inst" / inventory.NAME
                            ).read_bytes()
                        },
                    ),
                    f"required notice differs or is missing: yamaa/{inventory.NAME}",
                ),
            ):
                r_binary(path, replaced)
                try:
                    with (
                        self.subTest(asset=asset.title, defect=defect),
                        self.assertRaisesRegex(ValueError, error),
                    ):
                        self.select()
                finally:
                    path.write_bytes(original)
        self.assertEqual(len(self.select()), len(ASSETS))
        # The source package is never a built one.
        r_source = next(self.directory.rglob(f"yamaa_{R_VERSION}.tar.gz"))
        members = self.read(r_source)
        members["yamaa/DESCRIPTION"] += f"Built: {assets[0].built}; x; unix\n".encode()
        tar(r_source, members)
        with self.assertRaisesRegex(ValueError, "is a built package"):
            self.select()

    def test_command_line_archives_carry_their_executable_and_notices(self):
        """Each archive holds its target's executable and the command line's
        notices, each with its mode, and nothing else."""
        assets = [asset for asset in ASSETS if asset.kind == "cli"]
        self.assertEqual(len(assets), len(MATRIX.coverage()["cli"]["include"]))
        python_inventory = (ROOT / "python" / inventory.NAME).read_bytes()
        for asset in assets:
            path = self.directory / asset.artifact / example_name(asset, IDENTITY)
            target = asset.artifact.removeprefix("native-cli-")
            other = next(
                lane["target"]
                for lane in MATRIX.coverage()["cli"]["include"]
                if lane["target"] != target
            )
            original = path.read_bytes()
            with zipfile.ZipFile(path) as archive:
                members = {
                    entry.filename: (
                        archive.read(entry.filename),
                        entry.external_attr >> 16,
                    )
                    for entry in archive.infolist()
                }
            program, notices = cli_archive.layout(IDENTITY, target)
            directory = program.rpartition("/")[0]
            for defect, replaced, error in (
                (
                    "another target's executable",
                    dict(
                        members,
                        **{program: (executable(other), cli_archive.EXECUTABLE_MODE)},
                    ),
                    f"is not the yamaa executable for {target}",
                ),
                (
                    "an executable without its mode",
                    dict(
                        members,
                        **{program: (members[program][0], cli_archive.NOTICE_MODE)},
                    ),
                    "must be a regular file with Unix mode 755",
                ),
                (
                    "a missing notice",
                    {
                        name: value
                        for name, value in members.items()
                        if name != notices["LICENSE-YAML.txt"]
                    },
                    "must hold exactly",
                ),
                (
                    "another file",
                    dict(
                        members,
                        **{f"{directory}/README": (b"x\n", cli_archive.NOTICE_MODE)},
                    ),
                    "must hold exactly",
                ),
                (
                    "the wheels' inventory",
                    dict(
                        members,
                        **{
                            notices[inventory.NAME]: (
                                python_inventory,
                                cli_archive.NOTICE_MODE,
                            )
                        },
                    ),
                    f"required notice differs or is missing: {notices[inventory.NAME]}",
                ),
            ):
                cli_zip(path, replaced)
                try:
                    with (
                        self.subTest(asset=asset.title, defect=defect),
                        self.assertRaisesRegex(ValueError, error),
                    ):
                        self.select()
                finally:
                    path.write_bytes(original)
        self.assertEqual(len(self.select()), len(ASSETS))

    def test_the_conformance_archive_carries_the_committed_files(self):
        """The grammars with their README, the evaluation set and the license
        as the checkout holds them: an edited file fails the whole selection."""
        [asset] = [asset for asset in ASSETS if asset.kind == "conformance"]
        path = self.directory / asset.artifact / example_name(asset, IDENTITY)
        original = path.read_bytes()
        members = {
            name: (payload, conformance_archive.MODE)
            for name, payload in self.read(path).items()
        }
        file = "yaml/conformance/expressions.yaml"
        member = f"{path.name.removesuffix('.zip')}/{file}"
        payload, mode = members[member]
        cli_zip(path, dict(members, **{member: (payload + b"# edited\n", mode)}))
        try:
            with self.assertRaisesRegex(
                ValueError, f"{member} differs from {file} in the checkout"
            ):
                self.select()
        finally:
            path.write_bytes(original)
        self.assertEqual(len(self.select()), len(ASSETS))

    def read(self, path):
        """Read every regular member of a package archive."""
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                return {name: archive.read(name) for name in archive.namelist()}
        with tarfile.open(path) as archive:
            return {
                entry.name: archive.extractfile(entry).read()
                for entry in archive
                if entry.isfile()
            }

    def test_symlinks_or_subdirectories_are_not_release_assets(self):
        extra = self.directory / self.selected[0]["name"] / "unexpected"
        extra.mkdir()
        with self.assertRaisesRegex(ValueError, "artifact member"):
            self.select()


class QualifiedRunTrailer(unittest.TestCase):
    """The annotated release tag names the one native run it publishes."""

    def runs(self, messages):
        """Read the run each tag names, or the refusal, from one repository."""
        with tempfile.TemporaryDirectory() as directory:
            names = {f"tag-{index}": message for index, message in enumerate(messages)}
            tagged_repository(Path(directory), names)
            results = []
            for name in names:
                try:
                    results.append(release.qualified_run(Path(directory), name))
                except ValueError as error:
                    results.append(str(error))
            return results

    def test_the_closing_trailer_paragraph_names_the_run(self):
        messages = [
            "yamaa 1.0.0\n\nQualified-Run: 7",
            "yamaa 1.0.0\n\nqualified-run:7",
            "yamaa 1.0.0\n\nReviewed.\n\nReviewed-by: release test\nQualified-Run: 7",
        ]
        self.assertEqual(self.runs(messages), [7] * len(messages))

    def test_a_lightweight_tag_or_a_missing_or_malformed_trailer_is_refused(self):
        cases = [
            (None, "not an annotated tag"),
            # The subject paragraph is never a trailer paragraph.
            ("Qualified-Run: 7", "names no native run"),
            ("yamaa 1.0.0\n\nReviewed.", "names no native run"),
            ("yamaa 1.0.0\n\nReviewed-by: release test", "names no native run"),
            ("yamaa 1.0.0\n\nQualified-Run: 7\n\nReviewed.", "names no native run"),
            ("yamaa 1.0.0\n\nQualified-Run: 7\nreviewed", "names no native run"),
            ("yamaa 1.0.0\n\nQualified-Run:", "not: ''"),
            ("yamaa 1.0.0\n\nQualified-Run: 0", "not: '0'"),
            ("yamaa 1.0.0\n\nQualified-Run: 07", "not: '07'"),
            ("yamaa 1.0.0\n\nQualified-Run: -7", "not: '-7'"),
            ("yamaa 1.0.0\n\nQualified-Run: 7 8", "not: '7 8'"),
            ("yamaa 1.0.0\n\nQualified-Run: 7\nQualified-Run: 8", "not: '7', '8'"),
            ("yamaa 1.0.0\n\nQualified-Run: 7\nQualified-Run: 7", "not: '7', '7'"),
        ]
        results = self.runs([message for message, _ in cases])
        for (message, error), result in zip(cases, results, strict=True):
            with self.subTest(message=message):
                self.assertIsInstance(result, str)
                self.assertIn(error, result)


class PublicationBoundary(unittest.TestCase):
    def test_coordinated_version_and_committed_notes_are_required(self):
        self.assertEqual(release.prepared_release(ROOT, TAG), IDENTITY)
        # A rehearsal names no tag.
        self.assertEqual(release.prepared_release(ROOT, None), IDENTITY)
        for tag in (
            f"v{IDENTITY.version}",
            f"yamaa-v{OTHER_VERSION}",
            f"{TAG}-rc1",
            f"yamaa-v0{IDENTITY.version}",
        ):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.prepared_release(ROOT, tag)
        with (
            patch.object(release, "violations", return_value=["R version differs"]),
            self.assertRaisesRegex(ValueError, "incoherent"),
        ):
            release.prepared_release(ROOT, TAG)

    def test_annotated_tag_is_peeled_and_must_be_checked_out_on_main_ancestry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            git = functools.partial(support.git, support.init(root))
            (root / "file").write_text("main")
            git("add", "file")
            git("commit", "-m", "main")
            revision = git("rev-parse", "HEAD")
            git("update-ref", "refs/remotes/origin/main", revision)
            git("tag", "-a", TAG, "-m", "qualified release")
            self.assertEqual(release.tagged_revision(root, TAG), revision)
            # A rehearsal selects for the checkout, which must belong to main.
            self.assertEqual(release.checked_out_revision(root), revision)
            (root / "file").write_text("unqualified")
            git("commit", "-am", "side branch")
            with self.assertRaisesRegex(ValueError, "peeled tag"):
                release.tagged_revision(root, TAG)
            with self.assertRaisesRegex(ValueError, "main ancestry"):
                release.checked_out_revision(root)
            outside = f"yamaa-v{OTHER_VERSION}"
            git("tag", "-a", outside, "-m", "outside main")
            with self.assertRaisesRegex(ValueError, "main ancestry"):
                release.tagged_revision(root, outside)

    def publication(self, root, identity, tag=None):
        """Tag a checkout that prepares `identity`, select its assets, stub gh.

        Return the checkout's git command, the selected-assets directory and
        a function that publishes, returning the gh arguments or None.
        """
        tag = identity.tag if tag is None else tag
        checkout = root / "checkout"
        checkout.mkdir()

        git = functools.partial(support.git, support.init(checkout))
        declaration = checkout / release_identity.DECLARATION
        declaration.parent.mkdir(parents=True)
        declaration.write_text(
            f'version = "{identity.version}"\n'
            + (
                ""
                if identity.candidate is None
                else f"candidate = {identity.candidate}\n"
            )
        )
        (checkout / "file").write_text("qualified")
        git("add", ".")
        git("commit", "-m", "qualified")
        revision = git("rev-parse", "HEAD")
        git("tag", "-a", tag, "-m", "qualified")
        remote = root / "remote.git"
        git("clone", "--bare", str(checkout), str(remote))
        git("remote", "add", "origin", str(remote))
        artifacts = root / "qualified-release"
        artifacts.mkdir()
        (artifacts / "source-revision.txt").write_text(revision + "\n")
        assets = [artifacts / example_name(asset, identity) for asset in ASSETS]
        for asset in assets:
            asset.write_bytes(b"qualified package bytes")
        (artifacts / "asset-paths.txt").write_text(
            "".join(str(asset) + "\n" for asset in assets)
        )
        (artifacts / "release-notes.md").write_text("notes\n")
        commands = root / "bin"
        commands.mkdir()
        marker = root / "publication"
        gh = commands / "gh"
        gh.write_text(
            '#!/bin/sh\nfor argument in "$@"; do printf \'%s\\n\' "$argument"; done'
            ' > "$RELEASE_TEST_PUBLICATION"\n'
        )
        gh.chmod(0o755)
        environment = dict(
            os.environ,
            PATH=f"{commands}:{os.environ['PATH']}",
            RELEASE_TAG=tag,
            RUNNER_TEMP=str(root),
            GITHUB_REPOSITORY=REPOSITORY["full_name"],
            RELEASE_TEST_PUBLICATION=str(marker),
        )

        def publish():
            marker.unlink(missing_ok=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(checkout),
                    "update-ref",
                    "-d",
                    "refs/release-publication/checked-tag",
                ],
                check=False,
                stderr=subprocess.DEVNULL,
            )
            try:
                with patch.dict(os.environ, environment, clear=True):
                    publisher.publish(checkout, artifacts, tag, REPOSITORY["full_name"])
            except (subprocess.CalledProcessError, ValueError):
                self.assertFalse(marker.exists(), "a refused release must not publish")
                return None
            return marker.read_text().splitlines()

        return git, artifacts, publish

    def test_publication_rechecks_the_remote_annotated_tag_before_creating_a_release(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            git, artifacts, publish = self.publication(Path(directory), IDENTITY)
            arguments = publish()
            self.assertEqual(arguments[:3], ["release", "create", TAG])
            self.assertEqual(
                arguments[arguments.index("--title") + 1], f"yamaa v{IDENTITY.cargo}"
            )
            self.assertEqual(
                "--prerelease" in arguments, IDENTITY.candidate is not None
            )
            git("update-ref", "-d", "refs/release-publication/checked-tag")
            (Path(directory) / "checkout/file").write_text("moved tag")
            git("commit", "-am", "different source")
            git("tag", "-f", "-a", TAG, "-m", "moved")
            git("push", "--force", "origin", f"refs/tags/{TAG}")
            self.assertIsNone(
                publish(), "moved tags must not obtain release publication"
            )

    def test_a_candidate_is_published_as_a_pre_release_and_a_final_release_is_not(
        self,
    ):
        for identity, title, prerelease in [
            (Identity("1.0.0", 2), "yamaa v1.0.0-rc.2", True),
            (Identity("1.0.0"), "yamaa v1.0.0", False),
        ]:
            with (
                self.subTest(identity=str(identity)),
                tempfile.TemporaryDirectory() as directory,
            ):
                _, artifacts, publish = self.publication(Path(directory), identity)
                arguments = publish()
                self.assertEqual(arguments[:3], ["release", "create", identity.tag])
                self.assertEqual(arguments[arguments.index("--title") + 1], title)
                self.assertEqual("--prerelease" in arguments, prerelease)
                # Every qualified file keeps its name and receives its declared
                # title as a GitHub display label, in declaration order.
                self.assertEqual(
                    arguments[-len(ASSETS) :],
                    [
                        f"{artifacts / example_name(asset, identity)}#{asset.title}"
                        for asset in ASSETS
                    ],
                )

    def test_publication_refuses_any_asset_set_but_the_declared_one(self):
        identity = Identity("1.0.0", 1)
        for defect in ("missing", "extra", "renamed", "absent", "final names"):
            with (
                self.subTest(defect=defect),
                tempfile.TemporaryDirectory() as directory,
            ):
                _, artifacts, publish = self.publication(Path(directory), identity)
                listed = (artifacts / "asset-paths.txt").read_text().splitlines()
                if defect == "missing":
                    listed = listed[1:]
                elif defect == "extra":
                    extra = artifacts / "yamaa-1.0.0rc1-cp312-abi3-linux_x86_64.whl"
                    extra.write_bytes(b"unqualified")
                    listed.append(str(extra))
                elif defect == "renamed":
                    renamed = Path(listed[-1]).with_name("yamaa_1.0.0rc1.tar.gz")
                    Path(listed[-1]).rename(renamed)
                    listed[-1] = str(renamed)
                elif defect == "absent":
                    Path(listed[0]).unlink()
                else:
                    # The final release's names on a candidate's tag.
                    listed = [
                        str(artifacts / example_name(asset, Identity("1.0.0")))
                        for asset in ASSETS
                    ]
                    for name in listed:
                        Path(name).write_bytes(b"qualified package bytes")
                (artifacts / "asset-paths.txt").write_text(
                    "".join(name + "\n" for name in listed)
                )
                self.assertIsNone(publish())

    def test_publication_refuses_a_tag_the_tree_does_not_prepare(self):
        with tempfile.TemporaryDirectory() as directory:
            _, _, publish = self.publication(
                Path(directory), Identity("1.0.0", 2), tag="yamaa-v1.0.0-rc.1"
            )
            self.assertIsNone(publish())

    def test_real_selection_uses_paginated_attempt_jobs_and_never_rebuilds_or_publishes(
        self,
    ):
        producers = release_assets.artifacts(ASSETS).values()
        for label, listed_jobs, listed_artifacts, expected in [
            ("one attempt", native_jobs(), native_artifacts(), native_artifacts()),
            (
                "re-run all jobs",
                native_jobs(),
                native_artifacts(attempt=1) + native_artifacts(),
                native_artifacts(),
            ),
            (
                "re-run failed jobs",
                native_jobs(carried_over=producers),
                native_artifacts(attempt=1),
                native_artifacts(attempt=1),
            ),
        ]:
            # A release reads the run from its tag; a rehearsal is given it.
            for tag in (TAG, None):
                with (
                    self.subTest(label, tag=tag),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    self.check_selection(
                        Path(directory), listed_jobs, listed_artifacts, expected, tag
                    )

    def check_selection(self, root, listed_jobs, listed_artifacts, expected, tag):
        source, destination = root / "source", root / "selected"
        downloads(source)
        requests, commands = [], []

        def gh(*args):
            requests.append(args)
            endpoint = args[-1]
            if endpoint == f"repos/{REPOSITORY['full_name']}":
                return dict(REPOSITORY)
            if "/releases?" in endpoint:
                return [[], []]
            if endpoint == f"repos/{REPOSITORY['full_name']}/actions/runs/7":
                return native_run()
            if "/attempts/2/jobs?" in endpoint:
                return [{"jobs": listed_jobs[:10]}, {"jobs": listed_jobs[10:]}]
            if "/artifacts?" in endpoint:
                return [
                    {"artifacts": listed_artifacts[:2]},
                    {"artifacts": listed_artifacts[2:]},
                ]
            self.fail(endpoint)

        def download(command, **kwargs):
            commands.append(command)
            self.assertEqual(command[:2], ["gh", "api"])
            self.assertTrue(kwargs["check"])
            selected = next(
                artifact
                for artifact in expected
                if command[2]
                == f"repos/{REPOSITORY['full_name']}/actions/artifacts/{artifact['id']}/zip"
            )
            with zipfile.ZipFile(kwargs["stdout"], "w") as archive:
                for path in (source / selected["name"]).iterdir():
                    archive.writestr(path.name, path.read_bytes())

        with (
            patch.object(release, "tagged_revision", return_value=REVISION) as tagged,
            patch.object(release, "qualified_run", return_value=7) as named,
            patch.object(
                release, "checked_out_revision", return_value=REVISION
            ) as checkout,
            patch.object(release, "gh_json", side_effect=gh),
            patch.object(release.subprocess, "run", side_effect=download),
        ):
            assets = release.select_release(
                ROOT, REPOSITORY["full_name"], tag, destination, None if tag else 7
            )
        if tag is None:
            checkout.assert_called_once_with(ROOT)
            tagged.assert_not_called()
            named.assert_not_called()
        else:
            tagged.assert_called_once_with(ROOT, TAG)
            named.assert_called_once_with(ROOT, TAG)
            checkout.assert_not_called()
        endpoints = [request[-1] for request in requests]
        # The named run alone is read, never a list a later run could win.
        self.assertEqual(
            [endpoint for endpoint in endpoints if "/runs" in endpoint.split("?")[0]],
            [
                f"repos/{REPOSITORY['full_name']}/actions/runs/7",
                f"repos/{REPOSITORY['full_name']}/actions/runs/7/attempts/2/jobs?per_page=30",
                f"repos/{REPOSITORY['full_name']}/actions/runs/7/artifacts?per_page=100",
            ],
        )
        # Only a release looks for an existing one.
        self.assertEqual(
            any("/releases?" in endpoint for endpoint in endpoints), tag is not None
        )
        self.assertEqual(len(commands), len(release_assets.artifacts(ASSETS)))
        self.assertEqual(
            {command[2] for command in commands},
            {
                f"repos/{REPOSITORY['full_name']}/actions/artifacts/{artifact['id']}/zip"
                for artifact in expected
            },
        )
        self.assertIn(
            "/attempts/2/jobs?", "\n".join(request[-1] for request in requests)
        )
        self.assertEqual(
            (destination / "asset-paths.txt").read_text().splitlines(),
            [str(path.resolve()) for path in assets],
        )
        # The evidence follows the committed notes of the final version as a
        # section of its own. A candidate's introduction, whose text
        # CandidateReleaseBody pins, comes first.
        attempt = "https://github.com/elong0527/yamaa/actions/runs/7/attempts/2"
        notes = (
            (ROOT / f"docs/releases/{IDENTITY.version}.md")
            .read_text(encoding="utf-8")
            .rstrip("\n")
        )
        heading, _, body = notes.partition("\n\n")
        notes = (
            heading
            + "\n\n"
            + release.download_chooser(
                IDENTITY,
                REPOSITORY["full_name"],
                release_assets.match(assets, ASSETS, IDENTITY),
            )
            + "\n\n"
            + body
        )
        if IDENTITY.candidate is not None:
            introduction = release.candidate_introduction(
                ROOT, IDENTITY, REPOSITORY["full_name"]
            )
            notes = introduction + "\n\n" + notes
        self.assertEqual(
            (destination / "release-notes.md").read_text(encoding="utf-8"),
            notes
            + "\n\n## Qualified native artifacts\n\n"
            + f"Native qualification run: [{attempt}]({attempt}).\n\n"
            + "".join(
                f"- `{artifact['name']}` (artifact {artifact['id']})\n"
                for artifact in expected
            ),
        )
        self.assertEqual(
            (destination / "source-revision.txt").read_text(), REVISION + "\n"
        )
        for path in assets:
            self.assertEqual(
                path.read_bytes(),
                (source / path.parent.name / path.name).read_bytes(),
            )

    def test_existing_release_or_missing_qualification_never_downloads(self):
        for existing in (True, False):
            with self.subTest(existing=existing):
                with tempfile.TemporaryDirectory() as directory:
                    destination = Path(directory) / "selected"
                    replies = [
                        dict(REPOSITORY),
                        [[{"tag_name": TAG}]] if existing else [[]],
                        dict(native_run(), conclusion="failure"),
                    ]
                    with (
                        patch.object(release, "tagged_revision", return_value=REVISION),
                        patch.object(release, "qualified_run", return_value=7),
                        patch.object(release, "gh_json", side_effect=replies),
                        patch.object(release.subprocess, "run") as download,
                        self.assertRaises(ValueError),
                    ):
                        release.select_release(
                            ROOT, REPOSITORY["full_name"], TAG, destination
                        )
                    download.assert_not_called()
                    self.assertFalse(destination.exists())

    def test_a_named_run_that_cannot_qualify_is_refused_before_its_artifacts(self):
        name = REPOSITORY["full_name"]
        for key, value in [
            ("status", "in_progress"),
            ("conclusion", "failure"),
            ("head_sha", "b" * 40),
            ("path", ".github/workflows/ci.yml"),
            ("event", "pull_request"),
            ("head_repository", dict(REPOSITORY, full_name="fork/yamaa")),
            # GitHub answered for another run.
            ("id", 8),
        ]:
            for tag in (TAG, None):
                replies = {
                    f"repos/{name}": dict(REPOSITORY),
                    f"repos/{name}/releases?per_page=100": [[]],
                    f"repos/{name}/actions/runs/7": dict(native_run(), **{key: value}),
                }
                with (
                    self.subTest(key=key, tag=tag),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    destination = Path(directory) / "selected"
                    with (
                        patch.object(release, "tagged_revision", return_value=REVISION),
                        patch.object(release, "qualified_run", return_value=7),
                        patch.object(
                            release, "checked_out_revision", return_value=REVISION
                        ),
                        # Any other request, such as the run's jobs or
                        # artifacts, fails the test with a KeyError.
                        patch.object(
                            release,
                            "gh_json",
                            side_effect=lambda *args, replies=replies: replies[
                                args[-1]
                            ],
                        ),
                        patch.object(release.subprocess, "run") as download,
                        self.assertRaisesRegex(ValueError, "native run 7 cannot"),
                    ):
                        release.select_release(
                            ROOT, name, tag, destination, None if tag else 7
                        )
                    download.assert_not_called()
                    self.assertFalse(destination.exists())

    def test_a_tag_that_names_no_valid_run_is_refused_before_any_request(self):
        for message, error in [
            (None, "not an annotated tag"),
            ("yamaa 1.0.0\n\nReviewed.", "names no native run"),
            ("yamaa 1.0.0\n\nQualified-Run: 07", "one native run id"),
        ]:
            with (
                self.subTest(message=message),
                tempfile.TemporaryDirectory() as directory,
            ):
                checkout = Path(directory) / "checkout"
                checkout.mkdir()
                tagged_repository(checkout, {TAG: message})
                destination = Path(directory) / "selected"
                with (
                    patch.object(release, "prepared_release", return_value=IDENTITY),
                    patch.object(release, "gh_json") as gh,
                    patch.object(release, "download_artifact") as download,
                    self.assertRaisesRegex(ValueError, error),
                ):
                    release.select_release(
                        checkout, REPOSITORY["full_name"], TAG, destination
                    )
                gh.assert_not_called()
                download.assert_not_called()
                self.assertFalse(destination.exists())

    def test_the_tag_trailer_names_the_one_run_requested(self):
        name = REPOSITORY["full_name"]
        replies = {
            f"repos/{name}": dict(REPOSITORY),
            f"repos/{name}/releases?per_page=100": [[]],
            f"repos/{name}/actions/runs/9": dict(
                native_run(), id=9, conclusion="failure"
            ),
        }
        requests = []

        def gh(*args):
            requests.append(args[-1])
            return replies[args[-1]]

        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory) / "checkout"
            checkout.mkdir()
            tagged_repository(checkout, {TAG: "yamaa 1.0.0\n\nQualified-Run: 9"})
            with (
                patch.object(release, "prepared_release", return_value=IDENTITY),
                patch.object(release, "gh_json", side_effect=gh),
                self.assertRaisesRegex(ValueError, "native run 9 cannot"),
            ):
                release.select_release(
                    checkout, name, TAG, Path(directory) / "selected"
                )
        self.assertEqual(requests, list(replies))

    def test_selection_takes_a_tag_or_a_positive_rehearsal_run_id(self):
        for tag, run_id, error in [
            (TAG, 7, "either"),
            (None, None, "either"),
            (None, 0, "positive"),
            (None, True, "positive"),
        ]:
            with (
                self.subTest(tag=tag, run_id=run_id),
                tempfile.TemporaryDirectory() as directory,
                patch.object(release, "gh_json") as gh,
                self.assertRaisesRegex(ValueError, error),
            ):
                release.select_release(
                    ROOT,
                    REPOSITORY["full_name"],
                    tag,
                    Path(directory) / "selected",
                    run_id,
                )
            gh.assert_not_called()

    def test_the_command_takes_a_tag_or_a_rehearsal_run_id(self):
        command = [
            "--repository",
            REPOSITORY["full_name"],
            "--destination",
            "selected",
        ]
        for extra in ([], ["--tag", TAG, "--run-id", "7"], ["--run-id", "latest"]):
            with (
                self.subTest(extra=extra),
                patch.object(release, "select_release") as select,
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as stop,
            ):
                release.main(command + extra)
            self.assertEqual(stop.exception.code, 2)
            select.assert_not_called()
        for extra, tag, run_id in [
            (["--tag", TAG], TAG, None),
            (["--run-id", "7"], None, 7),
        ]:
            with (
                self.subTest(extra=extra),
                patch.object(release, "select_release", return_value=[]) as select,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                release.main(command + extra)
            select.assert_called_once_with(
                ROOT, REPOSITORY["full_name"], tag, Path("selected"), run_id
            )

    def test_wrong_default_branch_never_downloads(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "selected"
            with (
                patch.object(release, "tagged_revision", return_value=REVISION),
                patch.object(release, "qualified_run", return_value=7),
                patch.object(
                    release,
                    "gh_json",
                    return_value=dict(REPOSITORY, default_branch="other"),
                ),
                patch.object(release.subprocess, "run") as download,
                self.assertRaisesRegex(ValueError, "default branch"),
            ):
                release.select_release(ROOT, REPOSITORY["full_name"], TAG, destination)
            download.assert_not_called()
            self.assertFalse(destination.exists())


class ReleaseWorkflow(unittest.TestCase):
    def setUp(self):
        self.text = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.jobs = support.workflow("release.yml")["jobs"]

    def test_the_tag_publishes_the_github_release_and_nothing_else(self):
        # The owner uploads Python packages to PyPI from the release assets;
        # the workflow holds no PyPI step or credential (#3030 D57).
        self.assertEqual(list(self.jobs), ["publish"])
        self.assertNotIn("id-token", self.text)
        self.assertNotIn("pypi-publish", self.text)
        steps = self.jobs["publish"]["steps"]
        runs = [step.get("run", "") for step in steps]
        [selection] = [
            index for index, run in enumerate(runs) if "select_release_artifacts" in run
        ]
        [publication] = [
            index for index, run in enumerate(runs) if "tools.ci.publish_release" in run
        ]
        self.assertEqual(publication, selection + 1)
        self.assertEqual(publication, len(steps) - 1)
        self.assertIn(
            '--destination "${RUNNER_TEMP}/qualified-release"', runs[selection]
        )

    def test_the_selector_and_the_notice_check_import_without_pyyaml(self):
        # This workflow and the rehearsal run the selector on a bare
        # setup-python, and each native-r-binary job checks its binary's
        # notices without PyYAML. Without site-packages there is none, and
        # `import yaml` from the repository root finds its yaml/ directory, so
        # neither may import YAML at all (#3347).
        result = subprocess.run(
            [
                sys.executable,
                "-I",
                "-S",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); "
                "import tools.ci.select_release_artifacts; "
                "import tools.packaging.check_package_notices; "
                "print('yaml' in sys.modules)",
                str(ROOT),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "False\n")


class ReleaseRehearsalWorkflow(unittest.TestCase):
    def test_the_rehearsal_selects_with_its_read_only_token_and_never_publishes(self):
        workflow = support.workflow("release-rehearsal.yml")
        self.assertEqual(
            workflow["permissions"], {"contents": "read", "actions": "read"}
        )
        scripts = []
        for job in workflow["jobs"].values():
            self.assertNotIn("permissions", job)
            self.assertEqual(job["env"]["GH_TOKEN"], "${{ github.token }}")
            scripts.extend(step.get("run", "") for step in job["steps"])
        script = "\n".join(scripts)
        self.assertIn("python -m tools.ci.select_release_artifacts", script)
        self.assertIn("--run-id", script)
        for publication in ("publish_release", "gh release"):
            with self.subTest(publication=publication):
                self.assertNotIn(publication, script)


class ImmutableArtifactDownload(unittest.TestCase):
    def test_selected_id_download_preserves_package_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            payload = b"original package bytes\x00\xff"
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, "w") as archive:
                archive.writestr("package.whl", payload)

            def gh(command, **kwargs):
                self.assertEqual(
                    command,
                    [
                        "gh",
                        "api",
                        f"repos/{REPOSITORY['full_name']}/actions/artifacts/10/zip",
                    ],
                )
                kwargs["stdout"].write(stream.getvalue())

            with patch.object(release.subprocess, "run", side_effect=gh):
                release.download_artifact(
                    REPOSITORY["full_name"], native_artifacts()[0], destination
                )
            self.assertEqual(
                (
                    destination / native_artifacts()[0]["name"] / "package.whl"
                ).read_bytes(),
                payload,
            )
            self.assertFalse(list(destination.glob("*.zip")))

    def test_nested_traversal_link_directory_and_duplicate_artifact_members_fail(self):
        for defect in (
            "nested",
            "parent",
            "absolute",
            "backslash",
            "link",
            "directory",
            "duplicate",
            "fifo",
        ):
            with (
                self.subTest(defect=defect),
                tempfile.TemporaryDirectory() as directory,
            ):
                destination = Path(directory)
                stream = io.BytesIO()
                with zipfile.ZipFile(stream, "w") as archive:
                    names = {
                        "nested": "dir/package.whl",
                        "parent": "../package.whl",
                        "absolute": "/package.whl",
                        "backslash": "dir\\package.whl",
                        "directory": "dir/",
                    }
                    entry = zipfile.ZipInfo(names.get(defect, "package.whl"))
                    if defect in {"link", "fifo"}:
                        entry.create_system = 3
                        entry.external_attr = (
                            stat.S_IFLNK if defect == "link" else stat.S_IFIFO
                        ) << 16
                    archive.writestr(entry, b"payload")
                    if defect == "duplicate":
                        with self.assertWarns(UserWarning):
                            archive.writestr(entry, b"second")

                def gh(command, stream=stream, **kwargs):
                    kwargs["stdout"].write(stream.getvalue())

                with (
                    patch.object(release.subprocess, "run", side_effect=gh),
                    self.assertRaises(ValueError),
                ):
                    release.download_artifact(
                        REPOSITORY["full_name"], native_artifacts()[0], destination
                    )
                self.assertFalse(list(destination.rglob("*.whl")))
                self.assertFalse(list(destination.glob("*.zip")))


def candidate_tree(root, identity, tags=(), notes=True):
    """Commit a tree that prepares `identity` on main, after commits tagged `tags`.

    Each earlier tag gets its own commit, so it is an ancestor of the tree.
    """

    git = functools.partial(support.git, support.init(root))
    for name in tags:
        git("commit", "--allow-empty", "-m", name)
        git("tag", "-a", name, "-m", name)
    declaration = root / release_identity.DECLARATION
    declaration.parent.mkdir(parents=True, exist_ok=True)
    declaration.write_text(
        f'version = "{identity.version}"\n'
        + ("" if identity.candidate is None else f"candidate = {identity.candidate}\n")
    )
    (root / "docs/releases").mkdir(parents=True, exist_ok=True)
    (root / f"docs/releases/{identity.version}.md").write_text(
        f"# yamaa {identity.version}\n\nThe notes so far.\n"
    )
    if identity.candidate is not None and notes:
        (root / f"docs/releases/{identity.cargo}.md").write_text(
            "- A fix that this candidate brings.\n"
        )
    git("add", ".")
    git("commit", "-m", f"prepare {identity}")
    git("update-ref", "refs/remotes/origin/main", "HEAD")
    return git


class CandidateTags(unittest.TestCase):
    """A candidate tag publishes only a candidate, and a final tag only a final release."""

    def prepared(self, identity, tag, **tree):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_tree(root, identity, **tree)
            # The shared version check has its own tests; these trees hold
            # only the release identity and notes.
            with patch.object(release, "violations", return_value=[]):
                return release.prepared_release(root, tag)

    def test_a_candidate_tag_releases_the_candidate_the_tree_prepares(self):
        first, second = Identity("1.0.0", 1), Identity("1.0.0", 2)
        self.assertEqual(self.prepared(first, "yamaa-v1.0.0-rc.1"), first)
        self.assertEqual(
            self.prepared(second, "yamaa-v1.0.0-rc.2", tags=["yamaa-v1.0.0-rc.1"]),
            second,
        )
        # A rehearsal names no tag and checks the same tree.
        self.assertEqual(
            self.prepared(second, None, tags=["yamaa-v1.0.0-rc.1"]), second
        )
        final = Identity("1.0.0")
        self.assertEqual(
            self.prepared(final, "yamaa-v1.0.0", tags=["yamaa-v1.0.0-rc.1"]), final
        )

    def test_a_tag_of_the_other_kind_or_another_release_is_refused(self):
        for identity, tag, error in [
            (
                Identity("1.0.0"),
                "yamaa-v1.0.0-rc.1",
                "yamaa-v1.0.0-rc.1 is a candidate tag, but the tree prepares the "
                "final release 1.0.0; its tag is yamaa-v1.0.0",
            ),
            (
                Identity("1.0.0", 1),
                "yamaa-v1.0.0",
                "yamaa-v1.0.0 is a final release tag, but the tree prepares 1.0.0 "
                "release candidate 1; its tag is yamaa-v1.0.0-rc.1",
            ),
            (
                Identity("1.0.0", 1),
                "yamaa-v1.0.0-rc.2",
                "yamaa-v1.0.0-rc.2 names release 1.0.0 release candidate 2, but the "
                "tree prepares 1.0.0 release candidate 1",
            ),
            (
                Identity("1.0.0", 1),
                "yamaa-v1.0.1-rc.1",
                "names release 1.0.1 release candidate 1",
            ),
            (Identity("1.0.0", 1), "yamaa-v1.0.0rc1", "is not a release tag"),
        ]:
            with (
                self.subTest(identity=str(identity), tag=tag),
                self.assertRaisesRegex(ValueError, re.escape(error)),
            ):
                self.prepared(identity, tag)

    def test_a_candidate_needs_its_notes_and_follows_the_previous_candidate(self):
        second = Identity("1.0.0", 2)
        with self.assertRaisesRegex(
            ValueError, "candidate notes are missing: docs/releases/1.0.0-rc.2.md"
        ):
            self.prepared(
                second, "yamaa-v1.0.0-rc.2", tags=["yamaa-v1.0.0-rc.1"], notes=False
            )
        for tags in ([], ["yamaa-v1.0.0-rc.3"]):
            with (
                self.subTest(tags=tags),
                self.assertRaisesRegex(
                    ValueError,
                    "1.0.0 release candidate 2 must follow 1.0.0 release candidate "
                    "1, but its tag yamaa-v1.0.0-rc.1 is missing",
                ),
            ):
                self.prepared(second, "yamaa-v1.0.0-rc.2", tags=tags)

    def test_the_previous_candidate_must_be_an_ancestor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git = candidate_tree(root, Identity("1.0.0", 2))
            # Candidate 1 tagged on a commit that the tree does not contain.
            git("checkout", "-q", "--orphan", "elsewhere")
            git("commit", "-q", "--allow-empty", "-m", "elsewhere")
            git("tag", "-a", "yamaa-v1.0.0-rc.1", "-m", "elsewhere")
            git("checkout", "-q", "main")
            with (
                patch.object(release, "violations", return_value=[]),
                self.assertRaisesRegex(ValueError, "not an ancestor of this commit"),
            ):
                release.prepared_release(root, "yamaa-v1.0.0-rc.2")

    def test_a_wrong_kind_of_tag_is_refused_before_any_request_or_download(self):
        for identity, tag in [
            (Identity("1.0.0"), "yamaa-v1.0.0-rc.1"),
            (Identity("1.0.0", 1), "yamaa-v1.0.0"),
        ]:
            with (
                self.subTest(tag=tag),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory) / "checkout"
                root.mkdir()
                git = candidate_tree(root, identity)
                git("tag", "-a", tag, "-m", f"{tag}\n\nQualified-Run: 7")
                destination = Path(directory) / "selected"
                with (
                    patch.object(release, "violations", return_value=[]),
                    patch.object(release, "gh_json") as gh,
                    patch.object(release, "download_artifact") as download,
                    self.assertRaisesRegex(ValueError, "tag"),
                ):
                    release.select_release(
                        root, REPOSITORY["full_name"], tag, destination
                    )
                gh.assert_not_called()
                download.assert_not_called()
                self.assertFalse(destination.exists())


class CandidateReleaseBody(unittest.TestCase):
    def notes(self, identity, tags=()):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_tree(root, identity, tags=tags)
            return release.release_notes(
                root,
                identity,
                REPOSITORY["full_name"],
                native_run(),
                native_artifacts(),
                [(asset, Path(example_name(asset, identity))) for asset in ASSETS],
            )

    def test_a_candidate_says_what_it_is_where_feedback_goes_and_what_changed(self):
        body = self.notes(Identity("1.0.0", 2), tags=["yamaa-v1.0.0-rc.1"])
        for text in [
            "**This is release candidate 2 of yamaa 1.0.0, published for testing "
            "and feedback. It is not the final release.**",
            "https://github.com/elong0527/yamaa/issues/new/choose",
            "Install it from PyPI with `pip install --pre yamaa==1.0.0rc2`",
            # #3030 D53: R users reinstall the final release.
            "The R package is version `1.0.0` in every candidate and in the final "
            "release",
            "reinstall the R package when yamaa 1.0.0 is released.",
            "## Changes since release candidate 1\n\n- A fix that this candidate "
            "brings.\n\nEvery commit since release candidate 1: https://github.com/"
            "elong0527/yamaa/compare/yamaa-v1.0.0-rc.1...yamaa-v1.0.0-rc.2",
        ]:
            with self.subTest(text=text):
                self.assertIn(text, body)
        # The candidate warning stays ahead of the download chooser; the
        # committed notes and qualification evidence follow it.
        introduction, rest = body.split("\n\n# yamaa 1.0.0\n\n", 1)
        self.assertTrue(introduction.startswith("**This is release candidate 2"))
        self.assertTrue(rest.startswith("## Choose your download\n\n"))
        self.assertIn(
            "\n\nThe notes so far.\n\n## Qualified native artifacts\n\n", rest
        )

    def test_the_first_candidate_has_no_previous_candidate(self):
        body = self.notes(Identity("1.0.0", 1))
        self.assertIn(
            "## Release candidate 1\n\n- A fix that this candidate brings.\n\n"
            "# yamaa 1.0.0",
            body,
        )
        self.assertNotIn("/compare/", body)

    def test_a_final_release_body_has_downloads_before_its_notes_and_evidence(self):
        body = self.notes(Identity("1.0.0"), tags=["yamaa-v1.0.0-rc.1"])
        self.assertTrue(body.startswith("# yamaa 1.0.0\n\n## Choose your download\n\n"))
        self.assertIn(
            "\n\nThe notes so far.\n\n## Qualified native artifacts\n\n", body
        )
        self.assertNotIn("release candidate", body)

    def test_empty_candidate_notes_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_tree(root, Identity("1.0.0", 1))
            (root / "docs/releases/1.0.0-rc.1.md").write_text("\n \n")
            with self.assertRaisesRegex(ValueError, "candidate notes are empty"):
                release.release_notes(
                    root,
                    Identity("1.0.0", 1),
                    REPOSITORY["full_name"],
                    native_run(),
                    native_artifacts(),
                )

    def test_oversized_notes_or_download_chooser_never_write_publication_inputs(self):
        # The selection seam checks the notes before downloading and the complete
        # body before writing the files that authorize publication.
        name = REPOSITORY["full_name"]
        replies = {
            f"repos/{name}": dict(REPOSITORY),
            f"repos/{name}/releases?per_page=100": [[]],
            f"repos/{name}/actions/runs/7": native_run(),
            f"repos/{name}/actions/runs/7/attempts/2/jobs?per_page=30": [
                {"jobs": native_jobs()}
            ],
            f"repos/{name}/actions/runs/7/artifacts?per_page=100": [
                {"artifacts": native_artifacts()}
            ],
        }
        notes = release.release_notes(
            ROOT, IDENTITY, name, native_run(), native_artifacts()
        )
        for label, limit, downloads in [
            ("notes", 100, 0),
            ("chooser", len(notes), len(native_artifacts())),
        ]:
            with (
                self.subTest(label=label),
                tempfile.TemporaryDirectory() as directory,
            ):
                destination = Path(directory) / "selected"
                assets = [
                    destination / example_name(asset, IDENTITY) for asset in ASSETS
                ]
                with (
                    patch.object(release, "RELEASE_BODY_LIMIT", limit),
                    patch.object(release, "tagged_revision", return_value=REVISION),
                    patch.object(release, "qualified_run", return_value=7),
                    patch.object(
                        release, "gh_json", side_effect=lambda *args: replies[args[-1]]
                    ),
                    patch.object(release, "download_artifact") as download,
                    patch.object(release, "downloaded_assets", return_value=assets),
                    self.assertRaisesRegex(ValueError, "GitHub accepts at most"),
                ):
                    release.select_release(ROOT, name, TAG, destination)
                self.assertEqual(
                    (
                        download.call_count,
                        list(destination.glob("*.txt")),
                        list(destination.glob("*.md")),
                    ),
                    (downloads, [], []),
                    label,
                )


class ReleaseDownloadChooser(unittest.TestCase):
    def test_downloads_group_actual_file_names_and_pin_the_release_and_python_version(
        self,
    ):
        # The Markdown rendering seam uses qualified file names, including a
        # wheel's actual platform version, rather than declaration placeholders.
        for identity, python, cargo, pre in [
            (Identity("1.0.0"), "1.0.0", "1.0.0", ""),
            (Identity("1.0.0", 2), "1.0.0rc2", "1.0.0-rc.2", "--pre "),
        ]:
            with self.subTest(identity=identity):
                rows = [
                    ("r-source", "R source package", "yamaa_1.0.0.tar.gz"),
                    (
                        "cli",
                        "Command-line binary for Linux x86_64",
                        f"yamaa-{cargo}-x86_64-unknown-linux-musl.zip",
                    ),
                    ("sdist", "Python source archive", f"yamaa-{python}.tar.gz"),
                    (
                        "conformance",
                        "Grammars and expression evaluation set",
                        f"yamaa-{cargo}-conformance.zip",
                    ),
                    (
                        "r-binary",
                        "R binary package for Windows x64, R 4.6",
                        "yamaa_1.0.0-4.6.zip",
                    ),
                    (
                        "wheel",
                        "Python wheel for macOS arm64",
                        f"yamaa-{python}-cp312-abi3-macosx_14_0_arm64.whl",
                    ),
                ]
                pairs = [
                    (
                        release_assets.Asset(kind, title, name, "artifact", "job"),
                        Path("/qualified") / name,
                    )
                    for kind, title, name in rows
                ]
                site = "https://github.com/elong0527/yamaa"
                download = f"{site}/releases/download/yamaa-v{cargo}"
                self.assertEqual(
                    release.download_chooser(identity, "elong0527/yamaa", pairs),
                    "## Choose your download\n\n"
                    "### Python\n\n"
                    f"Install from PyPI with `python -m pip install {pre}"
                    f"yamaa=={python}`. pip selects a compatible wheel. "
                    "Building the source package requires Rust and a C compiler.\n\n"
                    f"- [Python source archive]({download}/yamaa-{python}.tar.gz)\n"
                    f"- [Python wheel for macOS arm64]({download}/"
                    f"yamaa-{python}-cp312-abi3-macosx_14_0_arm64.whl)\n\n"
                    "### R\n\n"
                    "Choose a binary for your operating system and R series; binaries "
                    "need no Rust or Rtools. The source package requires compilation. "
                    f"See the [R installation instructions]({site}/blob/yamaa-v{cargo}/"
                    "R/yamaa/README.md#installation).\n\n"
                    f"- [R source package]({download}/yamaa_1.0.0.tar.gz)\n"
                    "- [R binary package for Windows x64, R 4.6]"
                    f"({download}/yamaa_1.0.0-4.6.zip)\n\n"
                    "### CLI\n\n"
                    "Unzip the archive for your operating system and architecture, then "
                    "run `yamaa` (`yamaa.exe` on Windows). No Python, R or Rust "
                    "installation is needed.\n\n"
                    f"- [Command-line binary for Linux x86_64]({download}/"
                    f"yamaa-{cargo}-x86_64-unknown-linux-musl.zip)\n\n"
                    "### Developer resources\n\n"
                    "Grammars and conformance cases for language implementers.\n\n"
                    f"- [Grammars and expression evaluation set]({download}/"
                    f"yamaa-{cargo}-conformance.zip)",
                )

    def test_every_declared_asset_has_exactly_one_download_with_its_display_label(self):
        # The complete matrix must remain covered when a platform or asset is added.
        pairs = [(asset, Path(example_name(asset, IDENTITY))) for asset in ASSETS]
        chooser = release.download_chooser(IDENTITY, REPOSITORY["full_name"], pairs)
        self.assertCountEqual(
            re.findall(r"^- \[([^\]]+)\]\(([^)]+)\)$", chooser, re.MULTILINE),
            [
                (
                    asset.title,
                    f"https://github.com/elong0527/yamaa/releases/download/"
                    f"{IDENTITY.tag}/{example_name(asset, IDENTITY)}",
                )
                for asset in ASSETS
            ],
        )

    def test_an_asset_without_a_download_group_is_refused(self):
        # A new kind must be assigned a group before it can be published.
        asset = release_assets.Asset("unknown", "New asset", "new.zip", "build", "job")
        with self.assertRaisesRegex(ValueError, "no download group"):
            release.download_chooser(
                IDENTITY, "elong0527/yamaa", [(asset, Path("new.zip"))]
            )


class ReleaseAssetDeclaration(unittest.TestCase):
    def test_docs_list_exactly_the_declared_assets(self):
        lines = (ROOT / "docs/release.md").read_text(encoding="utf-8").splitlines()
        header = "| Asset | File | Native artifact |"
        self.assertEqual(lines.count(header), 1)
        start = lines.index(header) + 2
        rows = []
        for line in lines[start:]:
            if not line.startswith("|"):
                break
            rows.append(tuple(cell.strip() for cell in line.strip("|").split("|")))
        self.assertEqual(
            rows,
            [
                (asset.title, f"`{asset.file}`", f"`{asset.artifact}`")
                for asset in ASSETS
            ],
        )

    def test_a_name_pattern_spells_the_identity_for_each_package(self):
        wheel, sdist, r_source = (
            next(asset for asset in ASSETS if asset.kind == kind)
            for kind in ("wheel", "sdist", "r-source")
        )
        r_binary = next(
            asset
            for asset in ASSETS
            if asset.kind == "r-binary"
            and asset.producer
            == f"native-r-binary (windows-2025, R {MATRIX.R_RELEASE})"
        )
        series = MATRIX.r_series(MATRIX.R_RELEASE)
        older = MATRIX.r_series(MATRIX.R_OLDREL)
        candidate = Identity("1.0.0", 1)
        cli = next(
            asset
            for asset in ASSETS
            if asset.file.endswith("-x86_64-unknown-linux-musl.zip")
        )
        conformance = next(asset for asset in ASSETS if asset.kind == "conformance")
        for asset, accepted, refused in [
            (
                conformance,
                # The release's version, as its tag and title spell it.
                ["yamaa-1.0.0-rc.1-conformance.zip"],
                [
                    "yamaa-1.0.0rc1-conformance.zip",
                    "yamaa-1.0.0-conformance.zip",
                    "yamaa-1.0.0-rc.1-conformance.tar.gz",
                    "yamaa_1.0.0-rc.1-conformance.zip",
                ],
            ),
            (
                cli,
                # The engine's version, which `yamaa --version` prints.
                ["yamaa-1.0.0-rc.1-x86_64-unknown-linux-musl.zip"],
                [
                    "yamaa-1.0.0rc1-x86_64-unknown-linux-musl.zip",
                    "yamaa-1.0.0-x86_64-unknown-linux-musl.zip",
                    "yamaa-1.0.0-rc.1-aarch64-unknown-linux-musl.zip",
                    "yamaa-1.0.0-rc.1-x86_64-unknown-linux-gnu.zip",
                    "yamaa-1.0.0-rc.1-x86_64-unknown-linux-musl.tar.gz",
                ],
            ),
            (
                r_binary,
                [f"yamaa_1.0.0-{series}.zip"],
                [
                    "yamaa_1.0.0.zip",
                    f"yamaa_1.0.0rc1-{series}.zip",
                    f"yamaa_1.0.0-{older}.zip",
                    f"yamaa_1.0.0-{series}.tgz",
                ],
            ),
            (
                sdist,
                ["yamaa-1.0.0rc1.tar.gz"],
                [
                    "yamaa-1.0.0.tar.gz",
                    "yamaa-1.0.0-rc.1.tar.gz",
                    "yamaa-1x0x0rc1.tar.gz",
                ],
            ),
            (
                r_source,
                ["yamaa_1.0.0.tar.gz"],
                ["yamaa_1.0.0rc1.tar.gz", "yamaa_1.0.0-rc.1.tar.gz"],
            ),
            (
                wheel,
                [example_name(wheel, candidate)],
                [example_name(wheel, Identity("1.0.0"))],
            ),
        ]:
            pattern = asset.pattern(candidate)
            for name in accepted:
                with self.subTest(name=name):
                    self.assertTrue(pattern.fullmatch(name))
            for name in refused:
                with self.subTest(name=name):
                    self.assertFalse(pattern.fullmatch(name))

    def test_each_release_artifact_has_one_producer(self):
        coverage = MATRIX.coverage()
        binaries = len(coverage["r_binary"]["os"]) * len(coverage["r_binary"]["r"])
        # The Python source archive shares the source platform's wheel
        # artifact; the R source package and the conformance archive have
        # one each.
        self.assertEqual(
            len(release_assets.artifacts(ASSETS)),
            len(coverage["build"]["include"])
            + 1
            + binaries
            + len(coverage["cli"]["include"])
            + 1,
        )
        clash = release_assets.Asset("wheel", "clash", "x", ASSETS[0].artifact, "other")
        with self.assertRaisesRegex(ValueError, "two producing jobs"):
            release_assets.artifacts([*ASSETS, clash])

    def test_one_r_binary_per_platform_and_r_series(self):
        # #3030 D48: R-release and R-oldrel on Windows x64 and macOS arm64.
        coverage = MATRIX.coverage()
        expected = {
            (platform, version)
            for platform in coverage["r_binary"]["os"]
            for version in coverage["r_binary"]["r"]
        }
        binaries = [asset for asset in ASSETS if asset.kind == "r-binary"]
        self.assertEqual(len(binaries), len(expected))
        for asset in binaries:
            platform, version = re.fullmatch(
                r"native-r-binary \((\S+), R (\S+)\)", asset.producer
            ).groups()
            label, extension, r_platform = release_assets.R_BINARY_PLATFORMS[platform]
            series = MATRIX.r_series(version)
            with self.subTest(asset=asset.title):
                self.assertIn((platform, version), expected)
                self.assertEqual(
                    asset.title, f"R binary package for {label}, R {series}"
                )
                self.assertEqual(asset.file, f"yamaa_<R version>-{series}.{extension}")
                self.assertEqual(asset.built, f"R {version}; {r_platform}")
                self.assertEqual(
                    asset.artifact, f"native-r-binary-{platform}-{version}"
                )
        # Only R binary packages declare a build.
        self.assertEqual(
            {asset.kind for asset in ASSETS if asset.built is not None}, {"r-binary"}
        )

    def test_one_command_line_archive_per_target(self):
        # #3030 D12, D49: static musl binaries for Linux x86_64 and aarch64,
        # the static C runtime for Windows x64, and macOS arm64.
        lanes = MATRIX.coverage()["cli"]["include"]
        self.assertEqual(
            sorted((lane["os"], lane["target"]) for lane in lanes),
            [
                ("macos-15", "aarch64-apple-darwin"),
                ("ubuntu-24.04", "x86_64-unknown-linux-musl"),
                ("ubuntu-24.04-arm", "aarch64-unknown-linux-musl"),
                ("windows-2025", "x86_64-pc-windows-msvc"),
            ],
        )
        self.assertEqual(
            [
                (asset.title, asset.file, asset.artifact, asset.producer)
                for asset in ASSETS
                if asset.kind == "cli"
            ],
            [
                (
                    "Command-line binary for "
                    + release_assets.CLI_PLATFORMS[lane["os"]],
                    f"yamaa-<Cargo version>-{lane['target']}.zip",
                    f"native-cli-{lane['target']}",
                    f"native-cli ({lane['os']}, {lane['target']})",
                )
                for lane in lanes
            ],
        )

    def test_one_conformance_archive_built_on_the_source_platform(self):
        # Its files are the same for every platform (#3690).
        source = MATRIX.SOURCE_PLATFORM
        self.assertEqual(
            [
                (asset.title, asset.file, asset.artifact, asset.producer)
                for asset in ASSETS
                if asset.kind == "conformance"
            ],
            [
                (
                    "Grammars and expression evaluation set",
                    "yamaa-<Cargo version>-conformance.zip",
                    f"native-conformance-{source}",
                    f"native-conformance ({source})",
                )
            ],
        )

    def test_install_packages_reads_each_r_binary_name_as_the_package(self):
        # install.packages(path, repos = NULL) takes the package name from the
        # file name and reads that directory of the archive. R for Windows
        # drops ".zip" and then a final "_" with digits, dots and hyphens; R
        # for macOS drops ".tgz" and then everything from the first "_"
        # (utils:::.install.winbinary and utils:::.install.macbinary). Given
        # one file, install.packages() infers a Windows binary package from
        # ".zip" and a macOS one from ".tgz".
        rules = {
            ".zip": lambda name: re.sub(r"_[0-9.-]+$", "", name[: -len(".zip")]),
            ".tgz": lambda name: re.sub(r"_.*$", "", name[: -len(".tgz")]),
        }
        for identity in (IDENTITY, Identity("1.0.0", 1), Identity("1.0.0")):
            for asset in ASSETS:
                if asset.kind != "r-binary":
                    continue
                name = example_name(asset, identity)
                with self.subTest(name=name):
                    self.assertEqual(rules[Path(name).suffix](name), "yamaa")

    def test_matching_needs_one_file_per_asset_and_no_other_file(self):
        paths = [Path(example_name(asset, IDENTITY)) for asset in ASSETS]
        self.assertEqual(
            [asset for asset, _ in release_assets.match(paths, ASSETS, IDENTITY)],
            ASSETS,
        )
        for files, error in [
            (paths[1:], "missing or ambiguous release asset"),
            ([*paths, Path("notes.txt")], "no release asset declares: notes.txt"),
            ([*paths, paths[0]], "ambiguous"),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                release_assets.match(files, ASSETS, IDENTITY)


if __name__ == "__main__":
    unittest.main()
