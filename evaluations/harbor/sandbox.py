"""Linux verifier worker: submitted code cannot access grading assets.

The privileged grader prepares original inputs and executes a single script
as the unprivileged `nobody` account, which the base image already has, so
the image needs no change. Grading files and temporary reference outputs stay
private to root. Each rerun starts with clean output and worker scratch
directories.
"""

from __future__ import annotations

import os
import pwd
import shutil
import subprocess
from pathlib import Path

WORKER = "nobody"


def _remove(path: Path) -> None:
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink(missing_ok=True)


def prepare(app: Path, tests: Path, logs: Path) -> None:
    if os.geteuid() != 0:
        raise RuntimeError("the verifier sandbox must be prepared by root")
    pwd.getpwnam(WORKER)  # Fail before running any submitted code.
    tests.chmod(0o700)
    logs.mkdir(parents=True, exist_ok=True)
    logs.chmod(0o700)
    # Input artifacts are never trusted; restore the image's original fixtures.
    _remove(app / "input")
    shutil.copytree(tests / "input", app / "input")
    for path in [app / "input", *(app / "input").rglob("*")]:
        os.chown(path, 0, 0)
        path.chmod(0o555 if path.is_dir() else 0o444)
    os.chown(app, 0, 0)
    app.chmod(0o755)
    output = app / "output"
    if output.is_symlink():
        raise RuntimeError("the submission output directory must not be a symlink")
    output.mkdir(exist_ok=True)
    os.chown(output, 0, 0)
    # The sticky bit protects the root-owned script while allowing new outputs.
    output.chmod(0o1777)
    for path in output.iterdir():
        if path.is_symlink():
            raise RuntimeError(
                f"submission artifacts must not be symlinks: {path.name}"
            )
    shutil.copyfile(tests / "task.toml", logs / "task.toml")


def run(command: list[str], cwd: Path, timeout: float) -> tuple[int | None, str]:
    account = pwd.getpwnam(WORKER)
    source = Path(command[-1])
    output = cwd / "output"
    script = output / source.name
    original = script.read_bytes() if script.is_file() else None
    content = source.read_bytes()
    scratch = Path("/tmp/yamaa-submission")
    _remove(scratch)
    scratch.mkdir(mode=0o700)
    os.chown(scratch, account.pw_uid, account.pw_gid)
    for path in output.iterdir():
        _remove(path)
    script.write_bytes(content)
    os.chown(script, 0, 0)
    script.chmod(0o444)
    environment = {
        "PATH": os.environ["PATH"],
        "HOME": str(scratch),
        "TMPDIR": str(scratch),
        "LANG": "en_US.UTF-8",
    }
    try:
        done = subprocess.run(
            [
                "setpriv",
                "--reuid",
                str(account.pw_uid),
                "--regid",
                str(account.pw_gid),
                "--clear-groups",
                "--no-new-privs",
                *command[:-1],
                str(script),
            ],
            cwd=cwd,
            env=environment,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            start_new_session=True,
        )
        return done.returncode, (done.stdout + done.stderr)[-4000:]
    except subprocess.TimeoutExpired:
        return None, f"timed out after {timeout:.0f}s"
    finally:
        # Kill descendants too, including ones that created a new session.
        subprocess.run(["pkill", "-KILL", "-u", str(account.pw_uid)], check=False)
        if original is not None:
            script.write_bytes(original)
            script.chmod(0o444)
        _remove(scratch)
