"""PyPI must serve each release's Python packages with the release's bytes."""

import contextlib
import io
import json
import re
import subprocess
import unittest
import urllib.error
from unittest.mock import patch

from tools.ci import check_pypi_release as check
from tools.ci import release_assets
from tools.packaging.release_identity import Identity

REPOSITORY = "elong0527/yamaa"
# A candidate's Python version, 1.0.0rc2, makes it a pre-release on PyPI.
CANDIDATE = Identity("1.0.0", 2)
FINAL = Identity("1.0.0")
ASSETS = release_assets.declared()
FILES = "https://files.pythonhosted.org/packages/0a/1b/"


def example_name(asset, identity):
    """A file name the asset can have, with an example platform version."""
    return (
        asset.file.replace("<Python version>", identity.python)
        .replace("<R version>", identity.r)
        .replace("*", "11_0")
    )


def release(identity):
    """Each declared asset of the release, with bytes no other file has."""
    names = [example_name(asset, identity) for asset in ASSETS]
    return {name: b"qualified\0\xff" + name.encode() for name in names}


def packages(identity):
    """The Python packages of the release, which PyPI receives."""
    files = release(identity)
    return {
        name: files[name]
        for name in (
            example_name(asset, identity)
            for asset in ASSETS
            if asset.kind in release_assets.PYTHON_KINDS
        )
    }


class Services:
    """A GitHub release and a PyPI version, answering as gh and PyPI answer.

    `assets` maps each release asset's name to its bytes; `served` maps each
    file that PyPI lists for the version to its bytes, or is None when PyPI
    has no such version. Both services list digests that are wrong for every
    file, which the check never reads. A request that neither service answers
    fails the test.
    """

    def __init__(self, identity, assets, served):
        self.identity = identity
        self.assets = assets
        self.served = served
        self.ids = {name: 100 + index for index, name in enumerate(assets)}
        self.downloads = {"github": [], "pypi": []}

    def gh(self, *arguments):
        tag = self.identity.tag
        if arguments == ("api", f"repos/{REPOSITORY}/releases/tags/{tag}"):
            return json.dumps(
                {
                    "tag_name": tag,
                    "assets": [
                        {
                            "id": self.ids[name],
                            "name": name,
                            "size": len(payload),
                            "digest": "sha256:" + "0" * 64,
                        }
                        for name, payload in self.assets.items()
                    ],
                }
            ).encode()
        for name, identifier in self.ids.items():
            if arguments == (
                "api",
                "-H",
                "Accept: application/octet-stream",
                f"repos/{REPOSITORY}/releases/assets/{identifier}",
            ):
                self.downloads["github"].append(name)
                return self.assets[name]
        raise AssertionError(f"unexpected GitHub request: {arguments}")

    def fetch(self, url):
        if url == f"https://pypi.org/pypi/yamaa/{self.identity.python}/json":
            if self.served is None:
                raise urllib.error.HTTPError(url, 404, "Not Found", None, None)
            return json.dumps(
                {
                    "info": {"name": "yamaa", "version": self.identity.python},
                    "urls": [
                        {
                            "filename": name,
                            "url": FILES + name,
                            "digests": {"sha256": "0" * 64},
                        }
                        for name in self.served
                    ],
                }
            ).encode()
        name = url.removeprefix(FILES)
        if url.startswith(FILES) and name in (self.served or {}):
            self.downloads["pypi"].append(name)
            return self.served[name]
        raise AssertionError(f"unexpected PyPI request: {url}")


class PypiComparison(unittest.TestCase):
    def compare(self, services):
        with (
            patch.object(check, "gh", services.gh),
            patch.object(check, "fetch", services.fetch),
        ):
            return check.compare(REPOSITORY, services.identity.tag)

    def failure(self, services):
        with self.assertRaises(ValueError) as caught:
            self.compare(services)
        return str(caught.exception)

    def test_pypi_serves_each_python_package_with_the_release_bytes(self):
        for identity in (CANDIDATE, FINAL):
            with self.subTest(release=str(identity)):
                services = Services(identity, release(identity), packages(identity))
                names = list(packages(identity))
                self.assertEqual(self.compare(services), names)
                # Each package is read once from each side; the R source
                # package, which PyPI does not receive, is never read.
                self.assertEqual(services.downloads, {"github": names, "pypi": names})

    def test_a_package_pypi_lacks_or_any_other_file_there_fails(self):
        expected = packages(CANDIDATE)
        sdist = list(expected)[-1]
        without_sdist = {name: expected[name] for name in list(expected)[:-1]}
        other_wheel = f"yamaa-{CANDIDATE.python}-cp313-abi3-win_amd64.whl"
        [r_source] = set(release(CANDIDATE)) - set(expected)
        for served, messages in [
            (without_sdist, [f"PyPI does not serve {sdist}"]),
            ({**expected, other_wheel: b"other"}, [f"PyPI also serves {other_wheel}"]),
            (
                {**expected, r_source: release(CANDIDATE)[r_source]},
                [f"PyPI also serves {r_source}"],
            ),
            (
                {**without_sdist, other_wheel: b"other"},
                [f"PyPI does not serve {sdist}", f"PyPI also serves {other_wheel}"],
            ),
            ({}, ["PyPI does not serve " + ", ".join(expected)]),
        ]:
            with self.subTest(served=sorted(served)):
                services = Services(CANDIDATE, release(CANDIDATE), served)
                message = self.failure(services)
                for part in messages:
                    self.assertIn(part, message)
                # The file names are compared before any file is downloaded.
                self.assertEqual(services.downloads, {"github": [], "pypi": []})

    def test_other_bytes_fail_and_every_package_that_differs_is_named(self):
        expected = packages(CANDIDATE)
        names = list(expected)
        served = dict(expected)
        served[names[0]] = expected[names[0]][:-1]
        served[names[-1]] = expected[names[-1]].replace(b"\xff", b"\xfe")
        services = Services(CANDIDATE, release(CANDIDATE), served)
        message = self.failure(services)
        self.assertEqual(
            message,
            "PyPI serves other bytes than the GitHub release for "
            f"{names[0]}, {names[-1]}",
        )
        self.assertEqual(services.downloads["pypi"], names)

    def test_the_release_must_hold_exactly_the_declared_assets(self):
        assets = release(CANDIDATE)
        first = next(iter(assets))
        for held, error in [
            (
                {name: assets[name] for name in assets if name != first},
                "missing or ambiguous release asset",
            ),
            ({**assets, "notes.txt": b"notes"}, "no release asset declares: notes.txt"),
        ]:
            with self.subTest(error=error):
                services = Services(CANDIDATE, held, packages(CANDIDATE))
                with (
                    patch.object(check, "gh", services.gh),
                    patch.object(check, "fetch") as fetch,
                    self.assertRaisesRegex(
                        ValueError,
                        re.escape(f"the GitHub release of {CANDIDATE.tag}: {error}"),
                    ),
                ):
                    check.compare(REPOSITORY, CANDIDATE.tag)
                fetch.assert_not_called()
                self.assertEqual(services.downloads["github"], [])

    def test_a_version_pypi_does_not_serve_fails(self):
        services = Services(CANDIDATE, release(CANDIDATE), None)
        self.assertEqual(self.failure(services), "PyPI serves no yamaa 1.0.0rc2")

    def test_a_listing_of_another_shape_fails(self):
        name = list(packages(CANDIDATE))[0]
        for listing, error in [
            ({"info": {}}, "PyPI lists no files for yamaa 1.0.0rc2"),
            ([], "PyPI lists no files for yamaa 1.0.0rc2"),
            ({"urls": [{"filename": name}]}, "PyPI lists a malformed file"),
            ({"urls": ["file"]}, "PyPI lists a malformed file"),
            (
                {"urls": [{"filename": name, "url": FILES + name}] * 2},
                f"PyPI lists {name} twice",
            ),
        ]:
            with (
                self.subTest(error=error, listing=listing),
                patch.object(check, "fetch", return_value=json.dumps(listing).encode()),
                self.assertRaisesRegex(ValueError, re.escape(error)),
            ):
                check.pypi_files(CANDIDATE.python)
        tag = CANDIDATE.tag
        for found, error in [
            (
                {"tag_name": Identity("1.0.0", 1).tag, "assets": []},
                f"GitHub returned a release other than {tag}",
            ),
            ([], f"GitHub returned a release other than {tag}"),
            ({"tag_name": tag}, "lists no assets"),
            ({"tag_name": tag, "assets": [{"name": name}]}, "lists a malformed asset"),
            (
                {"tag_name": tag, "assets": [{"name": name, "id": "100"}]},
                "lists a malformed asset",
            ),
            (
                {"tag_name": tag, "assets": [{"name": name, "id": 0}]},
                "lists a malformed asset",
            ),
        ]:
            with (
                self.subTest(error=error, release=found),
                patch.object(check, "gh", return_value=json.dumps(found).encode()),
                self.assertRaisesRegex(ValueError, re.escape(error)),
            ):
                check.release_files(REPOSITORY, tag)

    def test_any_name_but_a_release_tag_or_repository_fails_before_any_request(self):
        for repository, tag, error in [
            (REPOSITORY, "yamaa-py-0.3.0", "is not a release tag"),
            (REPOSITORY, "v1.0.0", "is not a release tag"),
            (REPOSITORY, "yamaa-v1.0.0rc2", "is not a release tag"),
            ("elong0527/yamaa/releases", CANDIDATE.tag, "invalid release repository"),
        ]:
            with (
                self.subTest(repository=repository, tag=tag),
                patch.object(check, "gh") as gh,
                patch.object(check, "fetch") as fetch,
            ):
                with self.assertRaisesRegex(ValueError, error):
                    check.compare(repository, tag)
                gh.assert_not_called()
                fetch.assert_not_called()


class Transport(unittest.TestCase):
    def test_pypi_is_read_over_https_alone_with_a_time_limit(self):
        url = f"https://pypi.org/pypi/yamaa/{CANDIDATE.python}/json"
        with patch.object(
            check.urllib.request, "urlopen", return_value=io.BytesIO(b"body\0\xff")
        ) as urlopen:
            self.assertEqual(check.fetch(url), b"body\0\xff")
        urlopen.assert_called_once_with(url, timeout=check.TIMEOUT)
        for other in (
            "http://pypi.org/pypi/yamaa/json",
            "ftp://files.pythonhosted.org/packages/yamaa-1.0.0.tar.gz",
        ):
            with (
                self.subTest(url=other),
                patch.object(check.urllib.request, "urlopen") as urlopen,
            ):
                with self.assertRaisesRegex(ValueError, "not an HTTPS URL"):
                    check.fetch(other)
                urlopen.assert_not_called()

    def test_github_is_read_through_the_github_cli(self):
        arguments = ("api", f"repos/{REPOSITORY}/releases/tags/{CANDIDATE.tag}")
        completed = subprocess.CompletedProcess(["gh"], 0, stdout=b"listing\0\xff")
        with patch.object(check.subprocess, "run", return_value=completed) as run:
            self.assertEqual(check.gh(*arguments), b"listing\0\xff")
        run.assert_called_once_with(
            ["gh", *arguments], check=True, stdout=subprocess.PIPE
        )


class Command(unittest.TestCase):
    ARGV = ["check_pypi_release", "--repository", REPOSITORY, "--tag", CANDIDATE.tag]

    def test_the_command_lists_the_packages_it_compared(self):
        stdout = io.StringIO()
        with (
            patch("sys.argv", self.ARGV),
            patch.object(check, "compare", return_value=["a.whl", "a.tar.gz"]) as run,
            contextlib.redirect_stdout(stdout),
        ):
            check.main()
        run.assert_called_once_with(REPOSITORY, CANDIDATE.tag)
        self.assertEqual(
            stdout.getvalue(),
            f"PyPI serves the Python packages of {CANDIDATE.tag} unchanged:\n"
            "a.whl\na.tar.gz\n",
        )

    def test_the_command_fails_with_the_reason(self):
        for error in (
            ValueError("PyPI serves no yamaa 1.0.0rc2"),
            urllib.error.URLError("no network"),
            subprocess.CalledProcessError(1, ["gh", "api"]),
        ):
            stderr = io.StringIO()
            with (
                self.subTest(error=type(error).__name__),
                patch("sys.argv", self.ARGV),
                patch.object(check, "compare", side_effect=error),
                contextlib.redirect_stderr(stderr),
            ):
                with self.assertRaises(SystemExit) as caught:
                    check.main()
                self.assertEqual(caught.exception.code, 1)
                self.assertEqual(stderr.getvalue(), f"{error}\n")


if __name__ == "__main__":
    unittest.main()
