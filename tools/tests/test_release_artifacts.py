"""Release only complete native qualification and its unchanged tested packages."""

import contextlib
import copy
import io
import os
import stat
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

import yaml
from tools.ci import publish_release as publisher
from tools.ci import select_release_artifacts as release
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

MATRIX = release.native
VERSION = tomllib.loads((ROOT / "python/pyproject.toml").read_text(encoding="utf-8"))[
    "project"
]["version"]
TAG = f"yamaa-v{VERSION}"
OTHER_VERSION = (
    VERSION.rsplit(".", 1)[0] + "." + str(int(VERSION.rsplit(".", 1)[1]) + 1)
)
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
    producers, _ = release.release_artifacts(MATRIX.coverage())
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
    for arguments in (
        ("init", "-b", "main"),
        ("config", "user.name", "release test"),
        ("config", "user.email", "release-test@example.invalid"),
        ("commit", "--allow-empty", "-m", "qualified"),
        ("update-ref", "refs/remotes/origin/main", "HEAD"),
        *(
            ("tag", name) if message is None else ("tag", "-a", name, "-m", message)
            for name, message in messages.items()
        ),
    ):
        subprocess.run(
            ["git", "-C", str(root), *arguments],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def tar(path, members):
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in members.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(payload)
            archive.addfile(entry, io.BytesIO(payload))


def downloads(root):
    licenses = {
        path.name: path.read_bytes() for path in (ROOT / "python").glob("LICENSE-*.txt")
    }
    tags = ["manylinux_2_39_x86_64", "macosx_11_0_arm64", "win_amd64"]
    for platform, tag in zip(MATRIX.PLATFORMS, tags, strict=True):
        directory = root / f"native-python-{platform}-{MATRIX.BUILD_PYTHON}"
        directory.mkdir(parents=True)
        metadata = (
            f"Metadata-Version: 2.4\nName: yamaa\nVersion: {VERSION}\n\n".encode()
        )
        with zipfile.ZipFile(
            directory / f"yamaa-{VERSION}-cp312-abi3-{tag}.whl", "w"
        ) as wheel:
            wheel.writestr(f"yamaa-{VERSION}.dist-info/METADATA", metadata)
            wheel.writestr(
                "yamaa/_native.so", b"independent native fixture\0" + platform.encode()
            )
            for name, payload in licenses.items():
                wheel.writestr(f"yamaa-{VERSION}.dist-info/licenses/{name}", payload)
        tar(
            directory / f"yamaa-{VERSION}.tar.gz",
            {
                f"yamaa-{VERSION}/PKG-INFO": metadata,
                f"yamaa-{VERSION}/platform.txt": platform.encode(),
                **{
                    f"yamaa-{VERSION}/python/{name}": payload
                    for name, payload in licenses.items()
                },
            },
        )
    directory = root / "native-r-ubuntu-24.04"
    directory.mkdir(parents=True)
    tar(
        directory / f"yamaa_{VERSION}.tar.gz",
        {
            "yamaa/DESCRIPTION": f"Package: yamaa\nVersion: {VERSION}\n".encode(),
            **{f"yamaa/inst/{name}": payload for name, payload in licenses.items()},
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
        selected, linux = self.select(native_artifacts())
        self.assertEqual(linux, "ubuntu-24.04")
        self.assertEqual(len(selected), 4)
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
            self.assertEqual(self.select(artifacts)[0], qualifying)

    def test_a_failed_job_rerun_keeps_the_artifacts_of_the_producers_it_carried_over(
        self,
    ):
        producers, _ = release.release_artifacts(self.coverage)
        jobs = release.validate_jobs(
            native_jobs(carried_over=producers.values()), self.coverage, self.run
        )
        earlier = native_artifacts(attempt=1)
        self.assertEqual(self.select(earlier, jobs)[0], earlier)
        # A producer that attempt 2 did not run uploaded nothing during it.
        with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
            self.select(native_artifacts(), jobs)

    def test_an_artifact_counts_only_if_created_while_its_producer_ran(self):
        started, _, completed = ATTEMPTS[2]
        for created in (started, completed):
            artifacts = native_artifacts()
            artifacts[0]["created_at"] = created
            self.assertEqual(self.select(artifacts)[0], artifacts)
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
        producer = next(iter(release.release_artifacts(self.coverage)[0].values()))
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
            self.directory, self.selected, "ubuntu-24.04", VERSION, ROOT
        )

    def test_five_assets_keep_exact_bytes_and_choose_only_the_tested_linux_source_archives(
        self,
    ):
        before = {
            path: path.read_bytes()
            for path in self.directory.rglob("*")
            if path.is_file()
        }
        assets = self.select()
        self.assertEqual(len(assets), 5)
        self.assertEqual(sum(path.suffix == ".whl" for path in assets), 3)
        for path in assets:
            self.assertEqual(path.read_bytes(), before[path])
            if path.name.endswith(".tar.gz"):
                self.assertIn("ubuntu-24.04", path.parent.name)
        self.assertEqual(before, {path: path.read_bytes() for path in before})

    def test_filename_version_host_abi_and_extra_package_ambiguity_fail(self):
        wheel = next((self.directory / "native-python-ubuntu-24.04-3.14").glob("*.whl"))
        for bad in [
            wheel.name.replace(VERSION, OTHER_VERSION, 1),
            wheel.name.replace("abi3", "cp312"),
            wheel.name.replace("manylinux_2_39_x86_64", "win_amd64"),
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
        with self.assertRaisesRegex(ValueError, "one release wheel"):
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
                        "LICENSE-YAML.txt"
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
                        "LICENSE-YAML.txt"
                    ):
                        payload = b"not the required notices"
                    target.writestr(entry, payload)
            with self.subTest(defect=defect), self.assertRaises(ValueError):
                self.select()
            wheel.write_bytes(original)
        r_source = next((self.directory / "native-r-ubuntu-24.04").iterdir())
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
        self.assertEqual(release.release_version(ROOT, TAG), VERSION)
        # A rehearsal names no tag.
        self.assertEqual(release.release_version(ROOT, None), VERSION)
        for tag in (
            f"v{VERSION}",
            f"yamaa-v{OTHER_VERSION}",
            f"{TAG}-rc1",
            f"yamaa-v0{VERSION}",
        ):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.release_version(ROOT, tag)
        with (
            patch.object(release, "violations", return_value=["R version differs"]),
            self.assertRaisesRegex(ValueError, "incoherent"),
        ):
            release.release_version(ROOT, TAG)

    def test_annotated_tag_is_peeled_and_must_be_checked_out_on_main_ancestry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(root), *args],
                    text=True,
                    stderr=subprocess.DEVNULL,
                ).strip()

            git("init", "-b", "main")
            git("config", "user.name", "release test")
            git("config", "user.email", "release-test@example.invalid")
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

    def test_publication_rechecks_the_remote_annotated_tag_before_creating_a_release(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = root / "checkout"
            checkout.mkdir()

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(checkout), *args],
                    text=True,
                    stderr=subprocess.DEVNULL,
                ).strip()

            git("init", "-b", "main")
            git("config", "user.name", "release test")
            git("config", "user.email", "release-test@example.invalid")
            (checkout / "file").write_text("qualified")
            git("add", "file")
            git("commit", "-m", "qualified")
            revision = git("rev-parse", "HEAD")
            git("tag", "-a", TAG, "-m", "qualified")
            remote = root / "remote.git"
            git("clone", "--bare", str(checkout), str(remote))
            git("remote", "add", "origin", str(remote))
            artifacts = root / "qualified-release"
            artifacts.mkdir()
            (artifacts / "source-revision.txt").write_text(revision + "\n")
            assets = [artifacts / f"asset-{index}.whl" for index in range(5)]
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
                '#!/bin/sh\nprintf \'%s\\n\' "$*" > "$RELEASE_TEST_PUBLICATION"\n'
            )
            gh.chmod(0o755)
            environment = dict(
                os.environ,
                PATH=f"{commands}:{os.environ['PATH']}",
                RELEASE_TAG=TAG,
                RUNNER_TEMP=str(root),
                GITHUB_REPOSITORY=REPOSITORY["full_name"],
                RELEASE_TEST_PUBLICATION=str(marker),
            )

            def publish():
                try:
                    with patch.dict(os.environ, environment, clear=True):
                        publisher.publish(
                            checkout, artifacts, TAG, REPOSITORY["full_name"]
                        )
                except (subprocess.CalledProcessError, ValueError):
                    return False
                return True

            self.assertTrue(publish())
            self.assertIn(f"release create {TAG}", marker.read_text())
            marker.unlink()
            git("update-ref", "-d", "refs/release-publication/checked-tag")
            (checkout / "file").write_text("moved tag")
            git("commit", "-am", "different source")
            git("tag", "-f", "-a", TAG, "-m", "moved")
            git("push", "--force", "origin", f"refs/tags/{TAG}")
            self.assertFalse(publish())
            self.assertFalse(
                marker.exists(), "moved tags must not obtain release publication"
            )

    def test_real_selection_uses_paginated_attempt_jobs_and_never_rebuilds_or_publishes(
        self,
    ):
        producers = release.release_artifacts(MATRIX.coverage())[0].values()
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
                f"repos/{REPOSITORY['full_name']}/actions/runs/7/attempts/2/jobs?per_page=100",
                f"repos/{REPOSITORY['full_name']}/actions/runs/7/artifacts?per_page=100",
            ],
        )
        # Only a release looks for an existing one.
        self.assertEqual(
            any("/releases?" in endpoint for endpoint in endpoints), tag is not None
        )
        self.assertEqual(len(commands), 4)
        self.assertEqual(
            {command[2] for command in commands},
            {
                f"repos/{REPOSITORY['full_name']}/actions/artifacts/{artifact['id']}/zip"
                for artifact in expected
            },
        )
        self.assertTrue(any("/attempts/2/jobs?" in request[-1] for request in requests))
        self.assertEqual(
            (destination / "asset-paths.txt").read_text().splitlines(),
            [str(path.resolve()) for path in assets],
        )
        # The evidence follows the committed notes as a section of its own.
        attempt = "https://github.com/elong0527/yamaa/actions/runs/7/attempts/2"
        self.assertEqual(
            (destination / "release-notes.md").read_text(encoding="utf-8"),
            (ROOT / f"docs/releases/{VERSION}.md")
            .read_text(encoding="utf-8")
            .rstrip("\n")
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
                    patch.object(release, "release_version", return_value=VERSION),
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
                patch.object(release, "release_version", return_value=VERSION),
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
            "select_release_artifacts",
            "--repository",
            REPOSITORY["full_name"],
            "--destination",
            "selected",
        ]
        for extra in ([], ["--tag", TAG, "--run-id", "7"], ["--run-id", "latest"]):
            with (
                self.subTest(extra=extra),
                patch("sys.argv", command + extra),
                patch.object(release, "select_release") as select,
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as stop,
            ):
                release.main()
            self.assertEqual(stop.exception.code, 2)
            select.assert_not_called()
        for extra, tag, run_id in [
            (["--tag", TAG], TAG, None),
            (["--run-id", "7"], None, 7),
        ]:
            with (
                self.subTest(extra=extra),
                patch("sys.argv", command + extra),
                patch.object(release, "select_release", return_value=[]) as select,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                release.main()
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


class ReleaseRehearsalWorkflow(unittest.TestCase):
    def test_the_rehearsal_selects_with_its_read_only_token_and_never_publishes(self):
        workflow = yaml.load(
            (ROOT / ".github/workflows/release-rehearsal.yml").read_text(
                encoding="utf-8"
            ),
            Loader=UniqueKeyLoader,
        )
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


if __name__ == "__main__":
    unittest.main()
