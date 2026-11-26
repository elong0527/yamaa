"""The one compare-or-write step of the generators in tools/packaging.

A generator renders the bytes of each file it owns. By default it compares
the committed files with them and names each file that differs, is missing
or must go, then the command that regenerates them; with `--write` it writes
them. The comparison is a plain test, never an `assert`, so it holds under
`python -O` too.
"""

from tools.paths import ROOT


def shown(path):
    """A path as the messages name it: from the repository root, when it is
    inside it."""
    return path.relative_to(ROOT).as_posix() if path.is_relative_to(ROOT) else path


def stale(files, strays=()):
    """The paths of `files`, in its order, that are not regular files holding
    their bytes; a path of `strays` is reported as a stray instead.

    `files` maps each generated path to the bytes it must hold."""
    return [
        path
        for path, payload in files.items()
        if path not in strays and (not path.is_file() or path.read_bytes() != payload)
    ]


def store(files, strays=()):
    """Remove each of `strays`, then write each of `files`, its directory
    first. A stray goes first, so that nothing is written through a link."""
    for path in strays:
        path.unlink()
    for path, payload in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)


def sync(files, *, write, command, passed, strays=None, then=None):
    """Write `files` or compare the committed files with them; return the exit
    status.

    `files` maps each generated path to its bytes, and `strays` each path that
    must not be there to the reason, which `--write` removes. `command` is the
    generator's module, `passed` the sentence printed after `PASS:` when every
    file is current, and `then` the module of a generator that reads these
    files and must be run after it.
    """
    strays = strays or {}
    if write:
        store(files, strays)
        return 0
    errors = [f"ERROR: regenerate {shown(path)}" for path in stale(files, strays)]
    errors += [f"ERROR: {shown(path)} {reason}" for path, reason in strays.items()]
    if not errors:
        print(f"PASS: {passed}")
        return 0
    for error in errors:
        print(error)
    run = f"Run: python -m {command} --write"
    print(f"{run}, then python -m {then} --write" if then else run)
    return 1
