"""Select and download the unchanged artifacts of one complete native release run."""

from __future__ import annotations

import argparse
import email.parser
import json
import re
import shutil
import stat
import subprocess
import tarfile
import zipfile
from datetime import datetime
from pathlib import Path, PurePosixPath

from tools.ci import native, release_assets
from tools.packaging import (
    check_package_notices,
    cli_archive,
    conformance_archive,
    release_identity,
)
from tools.packaging.check_shared_release_versions import violations
from tools.paths import ROOT

# GitHub refuses a release body longer than this many characters.
RELEASE_BODY_LIMIT = 125_000


def require(condition, message):
    if not condition:
        raise ValueError(message)


def positive_integer(value):
    return type(value) is int and value > 0


def github_time(value, subject):
    """Parse a GitHub API timestamp, which is UTC to the second."""
    require(
        isinstance(value, str)
        and re.fullmatch(
            r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", value
        ),
        f"{subject}: missing or malformed GitHub timestamp",
    )
    return datetime.fromisoformat(value)


def prepared_release(root, tag):
    """Return the release identity the tree prepares, which `tag` must name.

    A rehearsal passes no tag. A candidate tag on a final release, or a final
    release tag on a candidate, is refused here, before any request or
    download.
    """
    errors = violations(root)
    require(not errors, "incoherent release versions: " + "; ".join(errors))
    identity = release_identity.read(root)
    if tag is not None:
        named = release_identity.from_tag(tag)
        require(
            named.candidate is None or identity.candidate is not None,
            f"{tag} is a candidate tag, but the tree prepares the final release "
            f"{identity}; its tag is {identity.tag}",
        )
        require(
            named.candidate is not None or identity.candidate is None,
            f"{tag} is a final release tag, but the tree prepares {identity}; "
            f"its tag is {identity.tag}",
        )
        require(
            named == identity,
            f"{tag} names release {named}, but the tree prepares {identity}; "
            f"its tag is {identity.tag}",
        )
    require(
        (root / f"docs/releases/{identity.version}.md").is_file(),
        "release notes are missing",
    )
    if identity.candidate is not None:
        require(
            (root / candidate_notes(identity)).is_file(),
            f"candidate notes are missing: {candidate_notes(identity)}",
        )
        previous = identity.previous()
        if previous is not None:
            require(
                reaches(root, f"refs/tags/{previous.tag}"),
                f"{identity} must follow {previous}, but its tag {previous.tag} "
                "is missing or not an ancestor of this commit",
            )
    return identity


def candidate_notes(identity):
    """The committed notes on what changed since the previous candidate."""
    return f"docs/releases/{identity.cargo}.md"


def git(root, *arguments):
    return subprocess.check_output(
        ["git", "-C", str(root), *arguments], text=True
    ).strip()


def reaches(root, revision, descendant="HEAD"):
    """Whether `revision` names a commit that `descendant` contains."""
    return (
        subprocess.run(
            [
                "git",
                "-C",
                str(root),
                "merge-base",
                "--is-ancestor",
                f"{revision}^{{commit}}",
                descendant,
            ],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode
        == 0
    )


def require_main_ancestry(root, revision, subject):
    require(
        reaches(root, revision, "origin/main"),
        f"{subject} must belong to current main ancestry",
    )


def tagged_revision(root, tag):
    revision = git(root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    require(re.fullmatch(r"[0-9a-f]{40}", revision), "tag has no commit identity")
    require(
        git(root, "rev-parse", "HEAD") == revision,
        "checkout is not the peeled tag commit",
    )
    require_main_ancestry(root, revision, "tag commit")
    return revision


def checked_out_revision(root):
    """Return the commit a rehearsal selects for: the checkout, on main."""
    revision = git(root, "rev-parse", "--verify", "HEAD^{commit}")
    require_main_ancestry(root, revision, "checked-out commit")
    return revision


TRAILER = re.compile(r"([A-Za-z0-9][A-Za-z0-9-]*)[ \t]*:(.*)")


def qualified_run(root, tag):
    """Return the native run id that the release tag's message names.

    The tag is annotated, and its message ends with a trailer paragraph:
    after a blank line, only `Key: value` lines, with exactly one
    `Qualified-Run: <run id>`. Keys compare without case, as in Git.
    """
    kind, _, body = git(
        root,
        "for-each-ref",
        "--format=%(objecttype)%0a%(contents:body)",
        f"refs/tags/{tag}",
    ).partition("\n")
    require(
        kind == "tag",
        f"{tag} is not an annotated tag; a release tag names its native run "
        "in a Qualified-Run: <run id> trailer",
    )
    # The body follows the subject paragraph and leaves out any signature.
    lines = body.splitlines()
    blanks = [index for index, line in enumerate(lines) if not line.strip()]
    start = blanks[-1] + 1 if blanks else 0
    trailers = [TRAILER.fullmatch(line) for line in lines[start:]]
    values = [
        match[2].strip()
        for match in trailers
        if match and match[1].lower() == "qualified-run"
    ]
    require(
        trailers and all(trailers) and values,
        f"{tag} names no native run; end its message with a trailer paragraph "
        "holding Qualified-Run: <run id>",
    )
    require(
        len(values) == 1 and re.fullmatch(r"[1-9][0-9]*", values[0]),
        f"{tag} must name one native run id in its Qualified-Run trailer, not: "
        + ", ".join(repr(value) for value in values),
    )
    return int(values[0])


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
        "release requires scheduled main, main dispatch or a same-repository qualification push",
    )
    return native.coverage()


def required_jobs(coverage):
    """Map each native job a release requires to the steps it must complete.

    These names, and the artifacts that `release_assets` declares, are the
    only copy: `tools/tests/test_native_workflow.py` checks them against
    native.yml.
    """
    jobs = {
        "portable-math-exact": (
            "Compare every candidate bit across the native matrix",
        ),
        "public-cohort-complete-runs": ("Compare complete installed host reports",),
    }
    for lane in coverage["rust"]["include"]:
        jobs[f"native-rust ({lane['os']}, {lane['python']})"] = (
            "Test shared workspace",
        )
    for lane in coverage["build"]["include"]:
        jobs[f"native-dist ({lane['os']}, {lane['python']})"] = (
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
    # Each Linux wheel, installed on glibc 2.28.
    for platform in coverage["glibc"]["os"]:
        jobs[f"native-glibc ({platform})"] = (
            "Install the wheel",
            "Report installed Python public benchmark conformance",
            "Qualify the installed package outside checkout",
        )
    # The source platform builds the R source archive once; every R lane
    # installs it.
    jobs[f"native-r-dist ({native.SOURCE_PLATFORM})"] = ("Build R source package",)
    # The source platform also builds the conformance archive once, with
    # the writer that checks it against the checkout.
    jobs[f"native-conformance ({native.SOURCE_PLATFORM})"] = (
        "Build the conformance archive",
    )
    for platform in coverage["r"]["os"]:
        jobs[f"native-r ({platform})"] = (
            "Install R outside checkout",
            "Report installed R public benchmark conformance",
            "Run installed R supplemental suites outside checkout",
            "Check R source package outside checkout",
        )
    # CRAN's incoming checks of the R source archive a release publishes.
    for platform in coverage["cran"]["os"]:
        for version in coverage["cran"]["r"]:
            jobs[f"native-cran ({platform}, R {version})"] = (
                "Check the R source package as CRAN does",
                "Confirm that the build used the minimum Rust",
            )
    # The same check with none of the suggested packages installed.
    for platform in coverage["cran_no_suggests"]["os"]:
        for version in coverage["cran_no_suggests"]["r"]:
            jobs[f"native-cran ({platform}, R {version}, no Suggests)"] = (
                "Confirm that no suggested package is installed",
                "Check the R source package as CRAN does",
                "Confirm that the build used the minimum Rust",
            )
    # The same check with the PDF and HTML manuals built and validated.
    for platform in coverage["cran_manual"]["os"]:
        for version in coverage["cran_manual"]["r"]:
            jobs[f"native-cran ({platform}, R {version}, manual)"] = (
                "Check the R source package as CRAN does",
                "Confirm that the build used the minimum Rust",
            )
    # The same check with the Rust of CRAN's Windows check.
    for platform in coverage["cran_rust"]["os"]:
        for version in coverage["cran_rust"]["r"]:
            for rust in coverage["cran_rust"]["rust"]:
                jobs[f"native-cran ({platform}, R {version}, Rust {rust})"] = (
                    "Check the R source package as CRAN does",
                    "Confirm that the build used CRAN's Rust",
                )
    for platform in coverage["cran_container"]["platform"]:
        jobs[f"native-cran ({platform})"] = ("Check the R source package as CRAN does",)
    # Each source archive that a release publishes, built and installed with
    # the network disabled on every platform, where the installed package
    # passes its selftest and then runs the conformance corpus.
    offline_steps = {
        "python": (
            "Install the Python source archive without a network",
            "Report installed Python public benchmark conformance",
        ),
        "r": (
            "Install the R source package without a network",
            "Report installed R public benchmark conformance",
        ),
    }
    for platform in coverage["offline_source"]["os"]:
        for host in coverage["offline_source"]["host"]:
            install, conformance = offline_steps[host]
            jobs[f"native-offline-source ({platform}, {host})"] = (
                install,
                "Run the installed package's selftest",
                conformance,
            )
    # The offline toolchain route on Linux: the minimum Rust, installed from
    # its standalone installer with no rustup and no network, then each
    # published source archive built and installed with it as above.
    for platform in coverage["offline_toolchain"]["os"]:
        for host in coverage["offline_toolchain"]["host"]:
            install, conformance = offline_steps[host]
            jobs[f"native-offline-toolchain ({platform}, {host})"] = (
                "Install Rust from its standalone installer without a network",
                install,
                "Run the installed package's selftest",
                conformance,
            )
    # Each wheel that a release publishes, installed from local files alone
    # with the network disabled, with the oldest and the newest supported
    # CPython, where the installed package passes its selftest.
    for platform in coverage["offline_wheel"]["os"]:
        for version in coverage["offline_wheel"]["python"]:
            jobs[f"native-offline-wheel ({platform}, {version})"] = (
                "Install the wheel without a network",
                "Run the installed package's selftest",
            )
    # Each R binary package that a release publishes, built from the R source
    # archive that it publishes, then installed on a runner with no Rust, no
    # Rtools and no network, where it passes its selftest and runs the
    # conformance corpus and the installed R suites.
    for platform in coverage["r_binary"]["os"]:
        for version in coverage["r_binary"]["r"]:
            jobs[f"native-r-binary ({platform}, R {version})"] = (
                "Build the R binary package",
                "Check the R binary package",
            )
            jobs[f"native-r-binary-install ({platform}, R {version})"] = (
                "Remove Rust and Rtools from the runner",
                "Install the R binary package without a network",
                "Run the installed package's selftest",
                "Report installed R public benchmark conformance",
                "Run installed R supplemental suites outside checkout",
            )
    # Each command-line archive that a release publishes, built on a runner
    # of its target's platform, then unpacked on a fresh one with the network
    # disabled, where the binary prints its version, passes its selftest and
    # runs the conformance corpus, still without a network.
    for lane in coverage["cli"]["include"]:
        label = f"({lane['os']}, {lane['target']})"
        jobs[f"native-cli {label}"] = (
            "Build the command-line binary",
            "Check the command-line binary",
            "Check the command-line archive",
        )
        jobs[f"native-cli-install {label}"] = (
            "Unpack the command-line archive without a network",
            "Run the command-line selftest",
            "Report command-line public benchmark conformance",
        )
    # Windows network shares: the installed wheel runs the conformance corpus
    # on an SMB share of the runner's own, through the route of each lane.
    for platform in coverage["share"]["os"]:
        for route in coverage["share"]["route"]:
            jobs[f"native-share ({platform}, {route})"] = (
                "Install the wheel",
                "Share a directory over SMB",
                "Report installed Python public benchmark conformance on the share",
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
    require(
        set(by_name) == set(expected),
        "native job set is incomplete or unexpected",
    )
    for name, job in by_name.items():
        require(
            job.get("status") == "completed" and job.get("conclusion") == "success",
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
    return by_name


def select_artifacts(artifacts, coverage, run, repository, jobs):
    """Select the artifact each producing job uploaded for the qualifying attempt.

    `jobs` maps names to the qualifying attempt's jobs. Artifacts carry no
    attempt number, and a re-run keeps every earlier attempt's artifacts on
    the run, so a name can appear once per attempt. The qualifying attempt
    lists each job with the start and completion of the execution it counts:
    the job's own re-run, or the earlier execution it carried over. A run is
    re-run only after it completes, so executions of one job never overlap,
    and the artifact created while its producer ran is the one that attempt
    qualified.
    """
    producers = release_assets.artifacts(release_assets.declared(coverage))
    selected = []
    for name, producer in producers.items():
        started = github_time(jobs[producer].get("started_at"), producer)
        completed = github_time(jobs[producer].get("completed_at"), producer)
        matches = [
            artifact
            for artifact in artifacts
            if artifact.get("name") == name
            and started <= github_time(artifact.get("created_at"), name) <= completed
        ]
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
    return selected


def package_version(payload, version):
    metadata = email.parser.BytesParser().parsebytes(payload, headersonly=True)
    require(
        metadata.get_all("Name") == ["yamaa"]
        and metadata.get_all("Version") == [version],
        "downloaded Python package metadata has a different or ambiguous name/version",
    )


def archive_payloads(path):
    wanted = {"METADATA", "PKG-INFO", "DESCRIPTION"}
    # A wheel, a Windows R binary package and a command-line archive are ZIP
    # archives; the source archives and a macOS R binary package are gzipped
    # tar archives.
    if path.suffix in {".whl", ".zip"}:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            require(len(names) == len(set(names)), "duplicate archive member")
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


def check_package(asset, path, identity, root):
    """Check one release file's own name, version and notices."""
    if asset.kind == "conformance":
        # Exactly the committed grammars, evaluation set and project
        # license, unchanged from the checkout. It is built from no crate,
        # so it carries no package notices.
        conformance_archive.check(path, root)
        return
    payloads = archive_payloads(path)
    if asset.kind in release_assets.PYTHON_KINDS:
        metadata = [
            payload
            for name, payload in payloads.items()
            if name.endswith(("/METADATA", "/PKG-INFO"))
        ]
        require(len(metadata) == 1, "Python package metadata is missing or ambiguous")
        package_version(metadata[0], identity.python)
    elif asset.kind in {"r-source", "r-binary"}:
        # R may write an installed DESCRIPTION with CRLF line ends on Windows.
        description = payloads.get("yamaa/DESCRIPTION", b"").decode("utf-8")
        description = description.replace("\r\n", "\n")
        require(
            re.findall(r"(?m)^Package: (.+)$", description) == ["yamaa"]
            and re.findall(r"(?m)^Version: (.+)$", description) == [identity.r],
            "R DESCRIPTION name/version differs",
        )
        # R CMD INSTALL --build records the R and the platform that built a
        # binary package, as `R 4.6.1; aarch64-apple-darwin20; <time>; unix`.
        built = re.findall(r"(?m)^Built: (.+)$", description)
        if asset.kind == "r-source":
            require(not built, "the R source package is a built package")
        else:
            require(
                len(built) == 1 and built[0].startswith(asset.built),
                f"R binary package was not built with {asset.built}",
            )
    elif asset.kind == "cli":
        # Exactly the executable of the archive's target and the notices, each
        # with its mode.
        cli_archive.check(path, root)
    else:
        raise ValueError(f"{asset.title}: no content check for {asset.kind} assets")
    # Each package carries the project license and every notice at its own
    # location, unchanged from the checkout.
    check_package_notices.check_archive(path, root)


def downloaded_assets(destination, selected, identity, root, assets):
    """Return the release files in declaration order, each checked.

    Every downloaded artifact must hold exactly the assets declared in it, so
    a missing, ambiguous or undeclared file fails the selection.
    """
    found = {}
    for artifact in selected:
        paths = sorted((destination / artifact["name"]).iterdir())
        require(
            all(path.is_file() and not path.is_symlink() for path in paths),
            "unexpected artifact member",
        )
        declared = [asset for asset in assets if asset.artifact == artifact["name"]]
        for asset, path in release_assets.match(paths, declared, identity):
            check_package(asset, path, identity, root)
            found[asset] = path
    require(set(found) == set(assets), "release assets are incomplete")
    published = [found[asset] for asset in assets]
    require(
        len({path.name for path in published}) == len(published),
        "release asset names are ambiguous",
    )
    return published


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


def select_release(root, repository_name, tag, destination, run_id=None):
    """Select, download and check the release assets of one native run.

    A release passes its tag, whose Qualified-Run trailer names the run. A
    rehearsal passes no tag and the run id instead: it selects for the
    checkout and does not look for an existing release. Neither publishes.
    """
    require(
        (tag is None) != (run_id is None),
        "name either a release tag or a rehearsal run id",
    )
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository_name),
        "invalid release repository",
    )
    identity = prepared_release(root, tag)
    if tag is None:
        require(positive_integer(run_id), "rehearsal run id must be positive")
        revision = checked_out_revision(root)
    else:
        revision = tagged_revision(root, tag)
        run_id = qualified_run(root, tag)
    repository = gh_json("api", f"repos/{repository_name}")
    require(
        repository.get("full_name") == repository_name
        and positive_integer(repository.get("id"))
        and repository.get("default_branch") == "main",
        "release repository identity or main default branch differs",
    )
    if tag is not None:
        releases = paginated(repository_name, "releases?per_page=100")
        require(
            not any(release.get("tag_name") == tag for release in releases),
            "release already exists; existing assets must not be overwritten",
        )
    # Only the named run counts, so a later run of the same commit cannot
    # replace the one the release was reviewed against.
    run = gh_json("api", f"repos/{repository_name}/actions/runs/{run_id}")
    try:
        require(
            isinstance(run, dict) and run.get("id") == run_id,
            "GitHub returned a different run",
        )
        coverage = run_coverage(run, repository, revision)
    except ValueError as error:
        raise ValueError(f"native run {run_id} cannot qualify: {error}") from None
    # Pages of 30, not GitHub's maximum of 100: GitHub lists the jobs of a
    # re-run attempt more slowly than a first attempt's, and a native run's
    # jobs at 100 per page pass its request time limit, so the listing fails
    # with HTTP 502. Re-running a job keeps a run eligible, so its final
    # attempt must still list.
    jobs = paginated(
        repository_name,
        f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=30",
        "jobs",
    )
    jobs = validate_jobs(jobs, coverage, run)
    artifacts = paginated(
        repository_name, f"actions/runs/{run['id']}/artifacts?per_page=100", "artifacts"
    )
    selected = select_artifacts(artifacts, coverage, run, repository, jobs)
    notes = release_notes(root, identity, repository_name, run, selected)
    require(
        len(notes) <= RELEASE_BODY_LIMIT,
        f"the release body has {len(notes):,} characters; GitHub accepts at most "
        f"{RELEASE_BODY_LIMIT:,}",
    )
    destination.mkdir(parents=True, exist_ok=False)
    for artifact in selected:
        download_artifact(repository_name, artifact, destination)
    declared = release_assets.declared(coverage)
    assets = downloaded_assets(destination, selected, identity, root, declared)
    (destination / "source-revision.txt").write_text(revision + "\n", encoding="utf-8")
    (destination / "asset-paths.txt").write_text(
        "".join(f"{path.resolve()}\n" for path in assets), encoding="utf-8"
    )
    (destination / "release-notes.md").write_text(notes, encoding="utf-8")
    return assets


def release_notes(root, identity, repository, run, selected):
    """Compose the release body from the committed notes and the native run.

    A candidate's body first says that it is a candidate, where feedback goes
    and what changed since the previous candidate.
    """
    notes = (root / f"docs/releases/{identity.version}.md").read_text(encoding="utf-8")
    notes = notes.rstrip("\n")
    if identity.candidate is not None:
        notes = candidate_introduction(root, identity, repository) + "\n\n" + notes
    attempt = f"{run['html_url']}/attempts/{run['run_attempt']}"
    # A section of its own, so the evidence does not read as part of the
    # committed notes' last section.
    notes += "\n\n## Qualified native artifacts\n\n"
    notes += f"Native qualification run: [{attempt}]({attempt}).\n\n"
    notes += "".join(
        f"- `{artifact['name']}` (artifact {artifact['id']})\n" for artifact in selected
    )
    return notes


def candidate_introduction(root, identity, repository):
    """Say what a candidate is, where feedback goes and what changed in it."""
    changes = (root / candidate_notes(identity)).read_text(encoding="utf-8").strip()
    require(changes, f"candidate notes are empty: {candidate_notes(identity)}")
    site = f"https://github.com/{repository}"
    final = identity.version
    lines = [
        f"**This is release candidate {identity.candidate} of yamaa {final}, "
        "published for testing and feedback. It is not the final release.**",
        "",
        "- **Feedback.** Report problems and comments in a GitHub issue: "
        f"{site}/issues/new/choose",
        "- **Python.** Install it from PyPI with "
        f"`pip install --pre yamaa=={identity.python}`; a bare "
        "`pip install yamaa` does not install a candidate reliably.",
        f"- **R.** The R package is version `{identity.r}` in every candidate and "
        "in the final release, because an R version cannot mark a candidate. R "
        "does not offer the final release as an update to a candidate, so "
        f"reinstall the R package when yamaa {final} is released.",
        "",
    ]
    previous = identity.previous()
    if previous is None:
        lines += [f"## Release candidate {identity.candidate}", "", changes]
    else:
        lines += [
            f"## Changes since release candidate {previous.candidate}",
            "",
            changes,
            "",
            f"Every commit since release candidate {previous.candidate}: "
            f"{site}/compare/{previous.tag}...{identity.tag}",
        ]
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--repository", required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument(
        "--tag", help="release tag, whose Qualified-Run trailer names the native run"
    )
    source.add_argument(
        "--run-id",
        type=int,
        help="rehearse a release of the checkout from this native run, without a tag",
    )
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        assets = select_release(
            args.root, args.repository, args.tag, args.destination, args.run_id
        )
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
