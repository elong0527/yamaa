"""Check "Supported platforms" in docs/release.md against the native matrix.

The section's table has one row per platform and one column per package form
that a release publishes. A cell claims what native qualification proves for
that form on that platform: the CPython or R series that a lane installs and
tests it with, the oldest glibc that a Linux wheel needs, or the Rust target
of the command-line binary. A cell that reads `no` claims nothing. This fails
when a cell claims something that no lane declared in tools/ci/native.py
qualifies, or when such a lane qualifies something that no cell claims, and
each message names the gap.

A lane qualifies a supported platform when it installs a form that a release
publishes on that platform, or builds and installs its source archive there,
and tests the installed package:

- native-qualify: the wheel and the Python source archive, with each CPython;
- native-glibc: each Linux wheel, on the oldest glibc that its manylinux tag
  admits;
- native-r, and native-cran with a released R: the R source package, with
  each R series;
- native-offline-source and native-offline-toolchain: both source archives,
  built with the network disabled. Their R is R-release, and native.py does
  not declare their CPython, so they qualify the Python source archive alone;
- native-offline-wheel: each wheel, installed from local files with the
  network disabled, with each CPython that it declares;
- native-r-binary-install: each R binary package, with its R series;
- native-cli-install: each command-line binary, for its Rust target.

The other lanes are diagnostic: native-rust tests the shared workspace,
native-dist builds what the qualifying lanes install, native-share runs the
conformance corpus with a wheel that native-qualify qualifies, on a Windows
network share, native-cran with R-devel checks the R source package with an R
that is not released yet, and its variants without the suggested packages,
with the PDF and HTML manuals, with the Rust of CRAN's Windows check, and in
R-hub's containers, gcc16, gcc-asan and clang-asan with their sanitizers, and
valgrind, repeat that check in more of CRAN's configurations. A
kind of lane that native.py adds fails this check until it is classified here.

An R source package cell may state a floor, `R 4.2 or later`, as the package's
DESCRIPTION does. It counts as qualified when lanes on its platform qualify the
floor and R-release, since no lane runs every series in between. Every other
cell lists its series, singly or as a range such as `3.12 to 3.15`.
"""

import re
import sys
from dataclasses import dataclass

from tools.ci import manylinux, native
from tools.paths import ROOT

PAGE = ROOT / "docs" / "release.md"
LABEL = "docs/release.md"
SECTION = "## Supported platforms"
MODULE = "tools/validation/check_supported_platforms.py"

# The platform that the page names for each runner of the native matrix.
PLATFORMS = {
    "ubuntu-24.04": "Linux x86_64",
    "ubuntu-24.04-arm": "Linux aarch64",
    "macos-15": "macOS arm64",
    "windows-2025": "Windows x64",
}

# The kinds of lane in native.coverage() that qualify a supported platform, and
# those that do not (see the module docstring).
QUALIFYING = frozenset(
    {
        "qualify",
        "glibc",
        "r",
        "cran",
        "offline_source",
        "offline_toolchain",
        "offline_wheel",
        "r_binary",
        "cli",
    }
)
DIAGNOSTIC = frozenset(
    {
        "rust",
        "build",
        "share",
        "cran_container",
        "cran_no_suggests",
        "cran_manual",
        "cran_rust",
    }
)


@dataclass(frozen=True)
class Column:
    """How the cells under one package form's heading are written.

    `noun` names the form in messages. `runtime` is the word before a cell's
    series, CPython or R; the command line's cells name a Rust target instead.
    `floor` admits a cell such as `R 4.2 or later`, and `glibc` a suffix such
    as `on glibc 2.28 or later`.
    """

    noun: str
    runtime: str | None = None
    floor: bool = False
    glibc: bool = False


WHEEL = "Python wheel"
PYTHON_SOURCE = "Python source archive"
R_SOURCE = "R source package"
R_BINARY = "R binary package"
CLI = "Command-line binary"
COLUMNS = {
    WHEEL: Column(WHEEL, "CPython", glibc=True),
    PYTHON_SOURCE: Column(PYTHON_SOURCE, "CPython"),
    R_SOURCE: Column(R_SOURCE, "R", floor=True),
    R_BINARY: Column(R_BINARY, "R"),
    CLI: Column("command-line binary"),
}
HEADER = ("Platform", *COLUMNS)
# The column of each form that native-qualify installs.
PYTHON_FORMS = {"wheel": WHEEL, "source": PYTHON_SOURCE}

SERIES = r"[0-9]+\.[0-9]+"
ITEM = re.compile(rf"({SERIES})(?: to ({SERIES}))?")
FLOOR = re.compile(rf"({SERIES}) or later")
GLIBC = re.compile(rf"(.+) on glibc ({SERIES}) or later")
TARGET = re.compile(r"`([0-9a-z_]+(?:-[0-9a-z_]+)+)`")

HINT = (
    "Make the Supported platforms table claim what the native lanes of "
    "tools/ci/native.py qualify: narrow a claim that no lane proves, or declare "
    "the lane that proves it, and add what a new lane proves to the table. "
    f"{MODULE} says which lanes qualify."
)


@dataclass(frozen=True)
class Lane:
    """What one native job qualifies.

    The job proves the package form `form` on `platform`. `kind` says what
    `value` is: "series" a CPython or R series, "glibc" the oldest glibc and
    "target" a Rust target; "form" proves the form alone, with no value.
    """

    job: str
    platform: str
    form: str
    kind: str
    value: str | None = None


@dataclass(frozen=True)
class Claim:
    """What one cell claims: the series that it lists or the floor that it
    states, and the oldest glibc or the Rust target that it names."""

    series: frozenset = frozenset()
    floor: str | None = None
    glibc: str | None = None
    target: str | None = None

    def covers(self, series):
        """Whether this cell claims `series`."""
        return series in self.series or (
            self.floor is not None and version(series) >= version(self.floor)
        )


def version(series):
    """A series or version as a comparable tuple, such as (4, 6) for 4.6."""
    return tuple(int(part) for part in series.split("."))


def listed(names):
    """Name things in prose: a, a and b, or a, b and c."""
    names = list(names)
    return " and ".join([", ".join(names[:-1]), names[-1]] if names[1:] else names)


def claim(column, text):
    """The Claim of one cell under `column`, or None for `no`.

    Raise ValueError when the cell is not written as its column admits.
    """
    spec = COLUMNS[column]
    if text == "no":
        return None
    if spec.runtime is None:
        target = TARGET.fullmatch(text)
        if target is None:
            raise ValueError("expected a Rust target in backquotes, or no")
        return Claim(target=target.group(1))
    glibc = None
    if spec.glibc and (suffix := GLIBC.fullmatch(text)):
        text, glibc = suffix.groups()
    runtime, _, written = text.partition(" ")
    if runtime != spec.runtime:
        raise ValueError(f"expected {spec.runtime} and its series, or no")
    if spec.floor and (floor := FLOOR.fullmatch(written)):
        return Claim(floor=floor.group(1), glibc=glibc)
    series = []
    for item in re.split(r", | and ", written):
        match = ITEM.fullmatch(item)
        if match is None:
            raise ValueError(f"{item!r} is neither a series nor a range of series")
        first, last = match.groups()
        if last is None:
            series.append(first)
            continue
        (major, low), (other, high) = version(first), version(last)
        if major != other or low >= high:
            raise ValueError(
                f"{item!r} does not run from an older to a newer series of one "
                "major version"
            )
        series += [f"{major}.{minor}" for minor in range(low, high + 1)]
    if len(set(series)) != len(series):
        raise ValueError("it names a series twice")
    return Claim(series=frozenset(series), glibc=glibc)


def cells(line):
    """The stripped cells of one Markdown table row."""
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def read(text):
    """The rows of the table under Supported platforms in the page `text`: each
    platform's cells by column, in table order.

    Raise ValueError unless the section holds one table whose first heading is
    Platform, with exactly HEADER's headings, a delimiter row, and one row of as
    many cells for each platform.
    """
    lines = text.splitlines()
    if lines.count(SECTION) != 1:
        raise ValueError(f"{LABEL}: expected one {SECTION!r} heading")
    start = lines.index(SECTION) + 1
    end = next(
        (index for index in range(start, len(lines)) if lines[index].startswith("## ")),
        len(lines),
    )
    section = lines[start:end]
    headings = [
        index
        for index, line in enumerate(section)
        if line.startswith("|") and cells(line)[0] == HEADER[0]
    ]
    if len(headings) != 1:
        raise ValueError(
            f"{LABEL}: expected one table under Supported platforms whose first "
            f"heading is {HEADER[0]}"
        )
    [index] = headings
    heading = tuple(cells(section[index]))
    if heading != HEADER:
        raise ValueError(
            f"{LABEL}: the Supported platforms table's headings are "
            f"{' | '.join(heading)}, not {' | '.join(HEADER)}"
        )
    body = []
    for line in section[index + 1 :]:
        if not line.startswith("|"):
            break
        body.append(cells(line))
    if (
        not body
        or len(body[0]) != len(HEADER)
        or not all(re.fullmatch(r":?-{3,}:?", cell) for cell in body[0])
    ):
        raise ValueError(
            f"{LABEL}: the Supported platforms table has no delimiter row under "
            "its headings"
        )
    rows = {}
    for row in body[1:]:
        if len(row) != len(HEADER):
            raise ValueError(
                f"{LABEL}: the Supported platforms row {' | '.join(row)} has "
                f"{len(row)} cells, not {len(HEADER)}"
            )
        platform, *values = row
        if platform in rows:
            raise ValueError(
                f"{LABEL}: Supported platforms has two rows for {platform}"
            )
        rows[platform] = dict(zip(COLUMNS, values, strict=True))
    if not rows:
        raise ValueError(f"{LABEL}: the Supported platforms table has no rows")
    return rows


def oldest_glibc():
    """The oldest glibc that the Linux wheels' manylinux tag admits, such as
    2.28, on which native-glibc installs them."""
    match = re.fullmatch(r"manylinux_([0-9]+)_([0-9]+)", manylinux.POLICY)
    if match is None:
        raise ValueError(
            f"tools/ci/manylinux.py: cannot read a glibc version in {manylinux.POLICY}"
        )
    return ".".join(match.groups())


def cran_series(r):
    """The R series of a native-cran lane's R, or None for R-devel, which is
    not a released series."""
    if r == "devel":
        return None
    if r == "release":
        return native.r_series(native.R_RELEASE)
    if r == "oldrel":
        return native.r_series(native.R_OLDREL)
    if re.fullmatch(rf"{SERIES}(?:\.[0-9]+)?", r):
        return native.r_series(r)
    raise ValueError(
        f"tools/ci/native.py: cannot tell the R series of native-cran's R {r}"
    )


def lanes(coverage=None):
    """What each qualifying lane of the native matrix proves, as Lanes.

    Raise ValueError for a kind of lane that is neither qualifying nor
    diagnostic, or for a runner that has no platform name.
    """
    coverage = native.coverage() if coverage is None else coverage
    unknown = sorted(set(coverage) - QUALIFYING - DIAGNOSTIC)
    if unknown:
        raise ValueError(
            f"tools/ci/native.py declares lanes that {MODULE} classifies neither "
            f"as qualifying nor as diagnostic: {', '.join(unknown)}"
        )
    release = native.r_series(native.R_RELEASE)
    found = []

    def add(job, runner, form, kind, value=None):
        if runner not in PLATFORMS:
            raise ValueError(f"{job}: {MODULE} names no platform for {runner}")
        found.append(Lane(job, PLATFORMS[runner], form, kind, value))

    for lane in coverage["qualify"]["include"]:
        runner, python, form = lane["os"], lane["python"], lane["form"]
        job = f"native-qualify ({runner}, {python}, {form})"
        if form not in PYTHON_FORMS:
            raise ValueError(f"{job}: {MODULE} does not know the Python form {form}")
        add(job, runner, PYTHON_FORMS[form], "series", python)
    glibc = oldest_glibc()
    for runner in coverage["glibc"]["os"]:
        add(f"native-glibc ({runner})", runner, WHEEL, "glibc", glibc)
    for runner in coverage["r"]["os"]:
        add(f"native-r ({runner})", runner, R_SOURCE, "series", release)
    for runner in coverage["cran"]["os"]:
        for r in coverage["cran"]["r"]:
            series = cran_series(r)
            if series is not None:
                add(
                    f"native-cran ({runner}, R {r})", runner, R_SOURCE, "series", series
                )
    for key, name in (
        ("offline_source", "native-offline-source"),
        ("offline_toolchain", "native-offline-toolchain"),
    ):
        for runner in coverage[key]["os"]:
            for host in coverage[key]["host"]:
                job = f"{name} ({runner}, {host})"
                if host == "python":
                    add(job, runner, PYTHON_SOURCE, "form")
                elif host == "r":
                    add(job, runner, R_SOURCE, "series", release)
                else:
                    raise ValueError(f"{job}: {MODULE} does not know the host {host}")
    for runner in coverage["offline_wheel"]["os"]:
        for python in coverage["offline_wheel"]["python"]:
            job = f"native-offline-wheel ({runner}, {python})"
            add(job, runner, WHEEL, "series", python)
    for runner in coverage["r_binary"]["os"]:
        for r in coverage["r_binary"]["r"]:
            add(
                f"native-r-binary-install ({runner}, R {r})",
                runner,
                R_BINARY,
                "series",
                native.r_series(r),
            )
    for lane in coverage["cli"]["include"]:
        runner, target = lane["os"], lane["target"]
        add(f"native-cli-install ({runner}, {target})", runner, CLI, "target", target)
    return found


def newest():
    """The newest CPython and R series that the native matrix declares."""
    return {
        "CPython": max(native.PYTHONS, key=version),
        "R": native.r_series(native.R_RELEASE),
    }


def qualifies(found):
    """`a qualifies`, or `a and b qualify`, for the jobs of the `found` lanes."""
    jobs = sorted({lane.job for lane in found})
    return f"{listed(jobs)} {'qualifies' if len(jobs) == 1 else 'qualify'}"


def problems(rows, found, newest):
    """Messages for each gap between the table's `rows` and the `found` lanes.

    `rows` maps each platform to its cells by column, as `read` returns them.
    `newest` maps CPython and R to the newest series that the matrix declares,
    which a floor needs qualified beside itself.
    """
    messages = []
    claims = {}
    for platform, values in rows.items():
        for column, text in values.items():
            try:
                claims[platform, column] = claim(column, text)
            except ValueError as error:
                messages.append(
                    f"{LABEL}: Supported platforms cannot read the "
                    f"{COLUMNS[column].noun} cell of {platform}, {text!r}: {error}"
                )
    by_cell = {}
    for lane in found:
        by_cell.setdefault((lane.platform, lane.form), []).append(lane)

    # What the table claims and no lane qualifies.
    for (platform, column), stated in claims.items():
        if stated is None:
            continue
        spec = COLUMNS[column]
        where = f"the {spec.noun} on {platform}"
        here = by_cell.get((platform, column), [])
        if not here:
            messages.append(
                f"{LABEL}: Supported platforms claims {where}, and no native lane "
                "qualifies it"
            )
            continue
        proved = {(lane.kind, lane.value) for lane in here}
        if stated.floor is not None:
            for series in dict.fromkeys((stated.floor, newest[spec.runtime])):
                if ("series", series) not in proved:
                    messages.append(
                        f"{LABEL}: Supported platforms claims {spec.runtime} "
                        f"{stated.floor} or later for {where}, and no native lane "
                        f"qualifies {spec.runtime} {series} there"
                    )
        for series in sorted(stated.series, key=version):
            if ("series", series) not in proved:
                messages.append(
                    f"{LABEL}: Supported platforms claims {spec.runtime} {series} "
                    f"for {where}, and no native lane qualifies it"
                )
        if stated.glibc is not None and ("glibc", stated.glibc) not in proved:
            messages.append(
                f"{LABEL}: Supported platforms claims glibc {stated.glibc} or later "
                f"for {where}, and no native lane qualifies it"
            )
        if stated.target is not None and ("target", stated.target) not in proved:
            messages.append(
                f"{LABEL}: Supported platforms names `{stated.target}` for {where}, "
                "and no native lane qualifies it"
            )

    # What a lane qualifies and the table does not claim.
    for platform in sorted({lane.platform for lane in found} - set(rows)):
        forms = [
            spec.noun
            for column, spec in COLUMNS.items()
            if (platform, column) in by_cell
        ]
        messages.append(
            f"{LABEL}: Supported platforms has no row for {platform}, where native "
            f"lanes qualify the {listed(forms)}"
        )
    for (platform, column), here in by_cell.items():
        if (platform, column) not in claims:
            continue
        spec = COLUMNS[column]
        where = f"the {spec.noun} on {platform}"
        stated = claims[platform, column]
        if stated is None:
            messages.append(
                f"{LABEL}: Supported platforms says no for {where}, which "
                f"{qualifies(here)}"
            )
            continue
        missing = {}
        for lane in here:
            if lane.kind == "series" and not stated.covers(lane.value):
                what = f"claim {spec.runtime} {lane.value}"
            elif lane.kind == "glibc" and stated.glibc != lane.value:
                what = f"claim glibc {lane.value} or later"
            elif lane.kind == "target" and stated.target != lane.value:
                what = f"name `{lane.value}`"
            else:
                continue
            missing.setdefault(what, []).append(lane)
        for what, unclaimed in missing.items():
            messages.append(
                f"{LABEL}: Supported platforms does not {what} for {where}, which "
                f"{qualifies(unclaimed)}"
            )
    return messages


def main():
    try:
        rows = read(PAGE.read_text(encoding="utf-8"))
        found = lanes()
    except ValueError as error:
        raise SystemExit(str(error)) from error
    messages = problems(rows, found, newest())
    for message in messages:
        print(message, file=sys.stderr)
    if messages:
        print(HINT, file=sys.stderr)
        raise SystemExit(1)
    print(
        f"{LABEL}: Supported platforms claims what {len(found)} qualifying native "
        f"lanes prove on {len(rows)} platforms, and nothing else"
    )


if __name__ == "__main__":
    main()
