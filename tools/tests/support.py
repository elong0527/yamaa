"""The helpers that the repository-tool tests share: scratch files, scratch git
repositories, the repository's own YAML and a command run in-process.

A helper here builds or reads and asserts nothing; the second test file that
needs a helper moves it here. `rules_fixture.py` keeps the rule tests'
fixture repository.
"""

import contextlib
import io
import subprocess
from pathlib import Path

import yaml
from tools.packaging import release_identity
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

# The configuration every scratch repository carries: one committer, and no
# signing, so a developer's own git configuration changes nothing a test
# commits or tags, nor what the tools under test commit in it.
GIT_CONFIG = """\
[user]
\tname = yamaa test
\temail = test@example.invalid
[commit]
\tgpgSign = false
[tag]
\tgpgSign = false
"""


def write(root, relative, data):
    """Write text, as UTF-8, or bytes at `root / relative`, creating its
    parent directories; return the path."""
    path = Path(root) / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(data, bytes):
        path.write_bytes(data)
    else:
        path.write_text(data, encoding="utf-8")
    return path


def git(root, *arguments):
    """Run git in `root` and return its standard output, without the trailing
    newline; a failing command raises CalledProcessError."""
    return subprocess.run(
        ["git", "-C", str(root), *arguments],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.rstrip("\n")


def init(root):
    """Make `root`, created when missing, an empty git repository on branch
    main with the scratch configuration; return it."""
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    git(root, "init", "-q", "-b", "main")
    with (root / ".git" / "config").open("a", encoding="utf-8") as config:
        config.write(GIT_CONFIG)
    return root


class Repository:
    """A scratch git repository whose files a test writes, commits and tags
    as a release."""

    def __init__(self, root):
        self.root = init(root)

    def write(self, path, data):
        return write(self.root, path, data)

    def edit(self, path, old, new):
        """Replace the one occurrence of `old` in a file."""
        text = (self.root / path).read_text(encoding="utf-8")
        if text.count(old) != 1:
            raise ValueError(f"{path} holds {old!r} {text.count(old)} times")
        self.write(path, text.replace(old, new))

    def remove(self, path):
        (self.root / path).unlink()

    def release(self, version, candidate=None):
        """Declare the release the tree prepares."""
        self.write(
            release_identity.DECLARATION,
            f'version = "{version}"\n'
            + ("" if candidate is None else f"candidate = {candidate}\n"),
        )

    def commit(self, message="change"):
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "--allow-empty", "-m", message)

    def tag(self, name):
        """Commit the tree as the release the tag names, and tag it."""
        identity = release_identity.from_tag(name)
        self.release(identity.version, identity.candidate)
        self.commit(name)
        git(self.root, "tag", "-a", name, "-m", name)


def load_yaml(path):
    """A YAML file, relative to the repository root or absolute, read as the
    repository checks read it."""
    return yaml.load((ROOT / path).read_text(encoding="utf-8"), Loader=UniqueKeyLoader)


def workflow(name):
    """One workflow of `.github/workflows`, by file name."""
    return load_yaml(Path(".github/workflows") / name)


def run_main(main, *arguments):
    """Run a command's `main(argv)` in-process; return its exit status, with
    success as 0, and what it printed to standard output and error."""
    output, errors = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
        try:
            status = main(list(arguments))
        except SystemExit as stopped:
            status = stopped.code
    return 0 if status is None else status, output.getvalue(), errors.getvalue()
