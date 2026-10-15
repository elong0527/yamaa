"""Select and download the unchanged artifacts of one complete native release run."""

from __future__ import annotations

import argparse
import email.parser
import importlib.util
import json
import re
import shutil
import stat
import subprocess
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

import tomllib
from check_shared_release_versions import violations

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "release_native_matrix", ROOT / ".github/scripts/ci/native_matrix.py"
)
native_matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native_matrix)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def positive_integer(value):
    return type(value) is int and value > 0


def release_version(root, tag):
    errors = violations(root)
    require(not errors, "incoherent release versions: " + "; ".join(errors))
    version = tomllib.loads((root / "python/pyproject.toml").read_text())["project"][
        "version"
    ]
    require(
        re.fullmatch(
            r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version
        )
        and tag == f"yamaa-v{version}",
        "tag must name the coordinated stable Python/R/Cargo version",
    )
    require(
        (root / f"docs/releases/{version}.md").is_file(), "release notes are missing"
    )
    return version


def tagged_revision(root, tag):
    def git(*arguments):
        return subprocess.check_output(
            ["git", "-C", str(root), *arguments], text=True
        ).strip()

    revision = git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    require(re.fullmatch(r"[0-9a-f]{40}", revision), "tag has no commit identity")
    require(
        git("rev-parse", "HEAD") == revision, "checkout is not the peeled tag commit"
    )
    require(
        subprocess.run(
            [
                "git",
                "-C",
                str(root),
                "merge-base",
                "--is-ancestor",
                revision,
                "origin/main",
            ],
            check=False,
        ).returncode
        == 0,
        "tag commit must belong to current main ancestry",
    )
    return revision


def run_coverage(run, repository, revision):
    require(
        run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and run.get("head_sha") == revision
        and isinstance(run.get("path"), str)
        and run["path"].split("@", 1)[0] == ".github/workflows/native.yml"
        and positive_integer(run.get("id"))
        and positive_integer(run.get("run_attempt")),
        "release requires a completed successful native run of the tag commit",
    )
    for owner in (run.get("repository"), run.get("head_repository")):
        require(
            isinstance(owner, dict)
            and owner.get("full_name") == repository["full_name"]
            and owner.get("id") == repository["id"],
            "native run repository or head repository differs from the release repository",
        )
    require(
        run.get("html_url")
        == f"https://github.com/{repository['full_name']}/actions/runs/{run['id']}",
        "native run URL differs from its repository/run identity",
    )
    event, branch = run.get("event"), run.get("head_branch")
    require(
        (event in {"schedule", "workflow_dispatch"} and branch == "main")
        or (
            event == "push"
            and isinstance(branch, str)
            and re.fullmatch(r"qualification/issue-[A-Za-z0-9][A-Za-z0-9._-]*", branch)
        ),
        "release requires nightly main, main dispatch or a same-repository qualification push",
    )
    coverage = native_matrix.plan(event, f"refs/heads/{branch}")
    require(
        coverage["full"] and coverage["run-rust"],
        "installed-only evidence cannot release",
    )
    return coverage


def required_jobs(coverage):
    jobs = {
        "native-coverage": ("Select the full installed matrix",),
        "portable-math-exact": (
            "Compare every candidate bit across the native matrix",
        ),
        "public-cohort-complete-runs": (
            "Require complete public runs through the M1 comparator",
        ),
    }
    for lane in coverage["build"]["include"]:
        suffix = f"({lane['os']}, {lane['python']})"
        jobs[f"native-rust {suffix}"] = (
            "Check shared crates and dependency direction",
        )
        jobs[f"native-dist {suffix}"] = (
            "Build wheel and source archive",
            "Check Python distribution contents",
        )
    for lane in coverage["qualify"]["include"]:
        jobs[f"native-qualify ({lane['os']}, {lane['python']}, {lane['form']})"] = (
            "Install the wheel"
            if lane["form"] == "wheel"
            else "Rebuild and install from the source archive",
            "Report installed Python public benchmark conformance",
            "Qualify the installed package outside checkout",
        )
    for platform in coverage["r"]["os"]:
        jobs[f"native-r ({platform})"] = (
            "Build and install R outside checkout",
            "Report installed R public benchmark conformance",
            "Run installed R supplemental suites outside checkout",
            "Require repository validator real-R public Check transport",
            "Check R source package outside checkout",
        )
    return jobs


def validate_jobs(jobs, coverage, run):
    expected = required_jobs(coverage)
    by_name = {}
    for job in jobs:
        name = job.get("name")
        require(
            isinstance(name, str) and name not in by_name,
            "duplicate or unnamed native job",
        )
        require(job.get("head_sha") == run["head_sha"], f"{name}: job source differs")
        require(
            job.get("run_attempt") == run["run_attempt"], f"{name}: job attempt differs"
        )
        by_name[name] = job
    # GitHub evaluates the whole-job condition before expanding this matrix.
    # Full shared Rust already runs these tools; the literal-name skip is intended.
    skipped = (
        {"native-numeric-tools (${{ matrix.os }})"} if coverage["run-rust"] else set()
    )
    require(
        set(by_name) == set(expected) | skipped,
        "native job set is incomplete or unexpected",
    )
    for name, job in by_name.items():
        require(
            job.get("status") == "completed"
            and job.get("conclusion") == ("skipped" if name in skipped else "success"),
            f"{name}: required native job did not succeed",
        )
        for required in expected.get(name, ()):
            matches = [
                step for step in job.get("steps", []) if step.get("name") == required
            ]
            require(
                len(matches) == 1
                and matches[0].get("status") == "completed"
                and matches[0].get("conclusion") == "success",
                f"{name}: required step did not succeed: {required}",
            )


def select_artifacts(artifacts, coverage, run, repository):
    names = [
        f"native-python-{lane['os']}-{lane['python']}"
        for lane in coverage["build"]["include"]
    ]
    linux = [
        platform for platform in coverage["r"]["os"] if platform.startswith("ubuntu-")
    ]
    require(len(linux) == 1, "canonical Linux source platform is ambiguous")
    names.append(f"native-r-{linux[0]}")
    selected = []
    for name in names:
        matches = [artifact for artifact in artifacts if artifact.get("name") == name]
        require(len(matches) == 1, f"missing or ambiguous native artifact: {name}")
        artifact = matches[0]
        owner = artifact.get("workflow_run", {})
        require(
            positive_integer(artifact.get("id"))
            and artifact.get("expired") is False
            and owner.get("id") == run["id"]
            and owner.get("head_sha") == run["head_sha"]
            and owner.get("repository_id") == repository["id"]
            and owner.get("head_repository_id") == repository["id"],
            f"{name}: expired artifact or different source/run/repository identity",
        )
        selected.append(artifact)
    require(
        len({artifact["id"] for artifact in selected}) == len(selected),
        "artifact IDs are ambiguous",
    )
    return selected, linux[0]


def package_version(payload, version):
    metadata = email.parser.BytesParser().parsebytes(payload, headersonly=True)
    require(
        metadata.get_all("Name") == ["yamaa"]
        and metadata.get_all("Version") == [version],
        "downloaded Python package metadata has a different or ambiguous name/version",
    )


def archive_payloads(path, license_names):
    wanted = {"METADATA", "PKG-INFO", "DESCRIPTION", *license_names}
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            require(len(names) == len(set(names)), "duplicate wheel member")
            return {
                name: archive.read(name)
                for name in names
                if PurePosixPath(name).name in wanted
            }
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        require(
            len({entry.name for entry in entries}) == len(entries),
            "duplicate source archive member",
        )
        return {
            entry.name: archive.extractfile(entry).read()
            for entry in entries
            if entry.isfile() and PurePosixPath(entry.name).name in wanted
        }


def downloaded_assets(destination, selected, linux, version, root):
    assets = []
    python_project = tomllib.loads((root / "python/pyproject.toml").read_text())[
        "project"
    ]
    for artifact in selected:
        directory = destination / artifact["name"]
        paths = list(directory.iterdir())
        require(
            all(path.is_file() and not path.is_symlink() for path in paths),
            "unexpected artifact member",
        )
        is_python = artifact["name"].startswith("native-python-")
        if is_python:
            wheels = [path for path in paths if path.suffix == ".whl"]
            sources = [path for path in paths if path.name == f"yamaa-{version}.tar.gz"]
            require(
                len(paths) == 2 and len(wheels) == len(sources) == 1,
                "expected one release wheel and source archive",
            )
            wheel = wheels[0]
            python_tag = "cp" + min(native_matrix.PYTHONS).replace(".", "")
            require(
                re.fullmatch(
                    rf"yamaa-{re.escape(version)}-{python_tag}-abi3-[A-Za-z0-9_.]+\.whl",
                    wheel.name,
                ),
                "wheel name/version/ABI differs",
            )
            platform_tag = wheel.name.removesuffix(".whl").rsplit("-", 1)[1]
            if artifact["name"].startswith("native-python-ubuntu-"):
                pattern = (
                    r"(?:manylinux(?:_[0-9]+_[0-9]+|1|2010|2014)_x86_64|linux_x86_64)"
                )
            elif artifact["name"].startswith("native-python-macos-"):
                pattern = r"macosx_[0-9]+_[0-9]+_arm64"
            elif artifact["name"].startswith("native-python-windows-"):
                pattern = r"win_amd64"
            else:
                raise ValueError("unsupported wheel host")
            require(
                all(re.fullmatch(pattern, tag) for tag in platform_tag.split(".")),
                "wheel platform differs from the qualified host",
            )
            assets.append(wheel)
            if (
                artifact["name"]
                == f"native-python-{linux}-{native_matrix.BUILD_PYTHON}"
            ):
                assets.append(sources[0])
        else:
            require(
                len(paths) == 1 and paths[0].name == f"yamaa_{version}.tar.gz",
                "R source name/version differs",
            )
            assets.append(paths[0])
        for path in paths:
            payloads = archive_payloads(path, python_project["license-files"])
            if is_python:
                metadata = [
                    payload
                    for name, payload in payloads.items()
                    if name.endswith(("/METADATA", "/PKG-INFO"))
                ]
                require(
                    len(metadata) == 1,
                    "Python package metadata is missing or ambiguous",
                )
                package_version(metadata[0], version)
            else:
                description = payloads.get("yamaa/DESCRIPTION", b"").decode("utf-8")
                require(
                    re.findall(r"(?m)^Package: (.+)$", description) == ["yamaa"]
                    and re.findall(r"(?m)^Version: (.+)$", description) == [version],
                    "R DESCRIPTION name/version differs",
                )
            for license_name in python_project["license-files"]:
                copies = [
                    payload
                    for name, payload in payloads.items()
                    if PurePosixPath(name).name == license_name
                ]
                require(
                    copies
                    and all(
                        payload == (root / "python" / license_name).read_bytes()
                        for payload in copies
                    ),
                    f"{path.name}: required license differs or is missing: {license_name}",
                )
    require(
        len(assets) == len(native_matrix.PLATFORMS) + 2
        and len({path.name for path in assets}) == len(assets),
        "release assets are incomplete or ambiguous",
    )
    return assets


def gh_json(*arguments):
    return json.loads(subprocess.check_output(["gh", *arguments], text=True))


def paginated(repository, endpoint, field=None):
    pages = gh_json("api", "--paginate", "--slurp", f"repos/{repository}/{endpoint}")
    require(isinstance(pages, list), "GitHub pagination did not return pages")
    rows = []
    for page in pages:
        values = page.get(field) if field and isinstance(page, dict) else page
        require(isinstance(values, list), "GitHub page is missing its collection")
        rows.extend(values)
    return rows


def download_artifact(repository, artifact, destination):
    """Download the selected immutable ID and extract only flat regular files."""
    directory = destination / artifact["name"]
    directory.mkdir()
    archive_path = destination / f"artifact-{artifact['id']}.zip"
    try:
        with archive_path.open("xb") as output:
            subprocess.run(
                [
                    "gh",
                    "api",
                    f"repos/{repository}/actions/artifacts/{artifact['id']}/zip",
                ],
                stdout=output,
                check=True,
            )
        with zipfile.ZipFile(archive_path) as archive:
            entries = archive.infolist()
            names = [entry.orig_filename for entry in entries]
            require(
                entries and len(set(names)) == len(names),
                "empty or ambiguous artifact ZIP",
            )
            require(
                all(
                    entry.orig_filename == entry.filename
                    and not entry.is_dir()
                    and stat.S_IFMT(entry.external_attr >> 16) in {0, stat.S_IFREG}
                    and re.fullmatch(r"[A-Za-z0-9_.-]+", entry.filename)
                    and entry.filename not in {".", ".."}
                    for entry in entries
                ),
                "artifact ZIP must contain only flat regular package files",
            )
            for entry in entries:
                with (
                    archive.open(entry) as source,
                    (directory / entry.filename).open("xb") as target,
                ):
                    shutil.copyfileobj(source, target)
    finally:
        archive_path.unlink(missing_ok=True)


def select_release(root, repository_name, tag, destination):
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository_name),
        "invalid release repository",
    )
    version = release_version(root, tag)
    revision = tagged_revision(root, tag)
    repository = gh_json("api", f"repos/{repository_name}")
    require(
        repository.get("full_name") == repository_name
        and positive_integer(repository.get("id"))
        and repository.get("default_branch") == "main",
        "release repository identity or main default branch differs",
    )
    releases = paginated(repository_name, "releases?per_page=100")
    require(
        not any(release.get("tag_name") == tag for release in releases),
        "release already exists; existing assets must not be overwritten",
    )
    runs = paginated(
        repository_name,
        f"actions/workflows/native.yml/runs?head_sha={revision}&status=success&per_page=100",
        "workflow_runs",
    )
    eligible = []
    for run in runs:
        try:
            coverage = run_coverage(run, repository, revision)
        except ValueError:
            continue
        eligible.append((run, coverage))
    require(
        eligible, "no completed successful full native run qualifies this tag commit"
    )
    require(
        len({run["id"] for run, _ in eligible}) == len(eligible),
        "native run identity is ambiguous",
    )
    run, coverage = max(eligible, key=lambda pair: pair[0]["id"])
    jobs = paginated(
        repository_name,
        f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100",
        "jobs",
    )
    validate_jobs(jobs, coverage, run)
    artifacts = paginated(
        repository_name, f"actions/runs/{run['id']}/artifacts?per_page=100", "artifacts"
    )
    selected, linux = select_artifacts(artifacts, coverage, run, repository)
    destination.mkdir(parents=True, exist_ok=False)
    for artifact in selected:
        download_artifact(repository_name, artifact, destination)
    assets = downloaded_assets(destination, selected, linux, version, root)
    (destination / "source-revision.txt").write_text(revision + "\n", encoding="utf-8")
    (destination / "asset-paths.txt").write_text(
        "".join(f"{path.resolve()}\n" for path in assets), encoding="utf-8"
    )
    notes = (root / f"docs/releases/{version}.md").read_text(encoding="utf-8")
    notes += f"\nQualified native artifacts: [{run['html_url']}/attempts/{run['run_attempt']}]({run['html_url']}/attempts/{run['run_attempt']}).\n"
    notes += (
        "\n".join(
            f"- `{artifact['name']}` (artifact {artifact['id']})"
            for artifact in selected
        )
        + "\n"
    )
    (destination / "release-notes.md").write_text(notes, encoding="utf-8")
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    try:
        assets = select_release(args.root, args.repository, args.tag, args.destination)
    except (
        ValueError,
        OSError,
        subprocess.CalledProcessError,
        zipfile.BadZipFile,
        tarfile.TarError,
    ) as error:
        parser.exit(1, f"{error}\n")
    print(
        "Selected unchanged qualified assets:\n"
        + "\n".join(path.name for path in assets)
    )


if __name__ == "__main__":
    main()
