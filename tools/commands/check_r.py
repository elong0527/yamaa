"""Check a standalone R archive outside the checkout as CRAN does.

`R CMD check --as-cran` must report nothing beyond the notes that
R/yamaa/cran-comments.md explains to CRAN. EXPECTED_NOTES lists them, with
the lines that each may print; any other NOTE, any WARNING or ERROR, or an
unexpected line under a listed NOTE fails the check. INFO findings, which R
does not count in its status, are printed and pass. With an R whose compiler
instruments packages with AddressSanitizer and UndefinedBehaviorSanitizer, a
sanitizer report in the installation, example or test output fails the check
too, since R passes a test that UndefinedBehaviorSanitizer reported on.

The check leaves out the PDF and HTML manuals (--no-manual), which need LaTeX
and HTML Tidy. With --manual it builds and validates them, as CRAN's incoming
checks do, and fails unless R reports both of those checks OK.
With --ignore-vignettes, the dependency-only check can run with no vignette
builders installed. Other checks continue to validate and rebuild vignettes.

    python -m tools.commands.check_r --archive <yamaa_X.Y.Z.tar.gz> --work-dir <new dir> [--manual] [--ignore-vignettes]
    python -m tools.commands.check_r --log <yamaa.Rcheck/00check.log> [--manual]

The second form compares the log of a check that ran elsewhere.
"""

import argparse
import dataclasses
import os
import re
import shutil
import subprocess
from pathlib import Path

from tools.paths import ROOT

# The notes that the package cannot remove, by check name, each with a
# pattern for every line that it may print. R/yamaa/cran-comments.md explains
# each one under "R CMD check results"; tools/tests/test_check_r.py keeps the
# two lists equal. To accept a new note, explain it there and add it here in
# the same change. A WARNING or ERROR is never accepted.
EXPECTED_NOTES = {
    # A first submission, and a tarball above CRAN's 10 MB guideline because
    # it bundles the crates that the build compiles (src/rust/vendor.tar.xz),
    # as CRAN's guidance for Rust asks. "New submission" needs CRAN's package
    # index, which a check without remote access does not read.
    "CRAN incoming feasibility": (
        r"Maintainer: .+",
        r"New submission",
        r"Size of tarball: \d+ bytes",
    ),
    # Above 5 MB, R 4.2 reports the installed size as a NOTE, and R 4.6.1 and
    # R-devel as INFO: the shared object links the Rust engine statically.
    # The tests' inputs stay in tests/, which R does not install.
    "installed package size": (
        r"installed size is +\d+\.\d+Mb",
        r"sub-directories of 1Mb or more:",
        r"libs +\d+\.\d+Mb",
    ),
}
# CRAN's Repository Policy: tarballs "should if possible not exceed 10MB", and
# "a modestly increased limit can be requested at submission" for bundled
# third-party sources. cran-comments.md requests this limit (a test keeps the
# two equal), and a larger tarball fails the check.
TARBALL_LIMIT = 15_000_000
TARBALL_SIZE = re.compile(r"Size of tarball: (\d+) bytes")
# On Unix, --as-cran runs the checkbashisms script over configure and cleanup,
# and warns instead when it is not installed, as on macOS runners and most
# development machines. CRAN's machines have it, and CI's Linux R jobs install
# it, so they run that check. Elsewhere this one warning, alone under its
# check, is reported and passes.
NO_CHECKBASHISMS = (
    "top-level files",
    (
        r"A complete check needs the .checkbashisms. script\.",
        r"See section .Configure and cleanup. in the .Writing R Extensions.(?: manual\.)?",
        r"manual\.",
    ),
)
# The checks that R runs only without --no-manual: the PDF manual, which
# pdflatex builds with the LaTeX packages and fonts that R's Rd.sty loads, and
# the validation of the HTML manual with HTML Tidy. R reports a failed PDF as
# a WARNING and a missing HTML Tidy as a NOTE, which fail anyway; a check with
# the manuals must also report both OK, so a log in which either did not run
# fails too.
MANUAL_CHECKS = ("PDF version of manual", "HTML version of manual")
# What CRAN's checks set beyond --as-cran: its incoming checks
# (check_CRAN_incoming.R) and, for the CPU limits, its regular checks
# (check.Renviron), both in CRAN/QA/Kurt in R's R-dev-web repository. The CPU
# limits flag an installation, example or test that keeps more than about two
# cores busy. The spelling check of DESCRIPTION runs only where aspell is.
CRAN_SETTINGS = {
    "_R_CHECK_CRAN_INCOMING_CHECK_FILE_URIS_": "true",
    "_R_CHECK_CRAN_INCOMING_NOTE_GNU_MAKE_": "true",
    "_R_CHECK_BASHISMS_EXTRA_": "true",
    "_R_CHECK_CODE_CLASS_IS_STRING_": "true",
    "_R_CHECK_DOC_SIZES2_": "true",
    "_R_CHECK_LENGTH_1_CONDITION_": "package:_R_CHECK_PACKAGE_NAME_,abort,verbose",
    "_R_CHECK_PACKAGE_DEPENDS_IGNORE_MISSING_ENHANCES_": "true",
    "_R_CHECK_RCPP_NOT_NEEDED_": "true",
    "_R_CHECK_UNDOC_USE_ALL_NAMES_": "true",
    "_R_CHECK_INSTALL_TIMING_CPU_TO_ELAPSED_THRESHOLD_": "2.5",
    "_R_CHECK_EXAMPLE_TIMING_CPU_TO_ELAPSED_THRESHOLD_": "2.5",
    "_R_CHECK_TEST_TIMING_CPU_TO_ELAPSED_THRESHOLD_": "2.5",
}
# What a sanitizer prints with each report. AddressSanitizer names itself and
# stops the program, so R fails the test or the installation.
# UndefinedBehaviorSanitizer reports and carries on: each report starts with
# "<location>: runtime error: ", and ends with the summary that names it,
# which R-hub's sanitizer containers search for, unless print_summary=0.
SANITIZER_REPORT = re.compile(
    r"AddressSanitizer|UndefinedBehaviorSanitizer|: runtime error: "
)
# The outputs of a check, below its yamaa.Rcheck, that hold what the package's
# compiled code prints: the installation, which loads the package, the
# examples and each test file.
SANITIZER_OUTPUTS = ("00install.out", "*-Ex.Rout", "tests/*.Rout", "tests/*.Rout.fail")
COUNTED = ("ERROR", "WARNING", "NOTE")
STATUSES = (*COUNTED, "INFO", "OK", "SKIPPED", "NONE")
HEADER = re.compile(r"^\* checking (?P<check>.+?) \.\.\.(?P<rest>.*)$")
RESULT = re.compile(r"^\s*(?:\[[^\]]*\]\s*)?(?P<status>" + "|".join(STATUSES) + r")$")
STATUS = re.compile(r"^Status: (?P<summary>.+)$")


@dataclasses.dataclass
class Finding:
    check: str
    status: str
    lines: list


def findings(log):
    """Read every check result of a 00check.log, with the lines under it.

    Returns the findings and the counts that the final Status line gives.
    """
    found, summary, current = [], None, None
    for line in log.splitlines():
        if line.startswith("* ") or STATUS.match(line):
            current = None
            if match := STATUS.match(line):
                summary = match["summary"]
            elif match := HEADER.match(line):
                result = RESULT.match(match["rest"])
                current = Finding(
                    match["check"], result["status"] if result else None, []
                )
                found.append(current)
            continue
        if current is None:
            continue
        if current.status is None and (result := RESULT.match(line)):
            current.status = result["status"]
        elif current.status is not None and line.strip():
            current.lines.append(line.strip())
    if summary is None:
        raise ValueError("the check log has no Status line")
    counts = dict.fromkeys(COUNTED, 0)
    if summary != "OK":
        for part in summary.split(", "):
            match = re.fullmatch(r"(\d+) (ERROR|WARNING|NOTE)s?", part)
            if match is None:
                raise ValueError(f"unreadable check status: {summary}")
            counts[match[2]] = int(match[1])
    return found, counts


def matches(lines, patterns):
    return all(any(re.fullmatch(p, line) for p in patterns) for line in lines)


def lacks_checkbashisms(finding):
    """Whether a finding is R's warning that checkbashisms is not installed."""
    return (
        finding.status == "WARNING"
        and finding.check == NO_CHECKBASHISMS[0]
        and matches(finding.lines, NO_CHECKBASHISMS[1])
    )


def unexpected(log, expected=EXPECTED_NOTES, checkbashisms=True, manual=False):
    """Describe every finding of a check log that `expected` does not allow.

    `checkbashisms` says whether the check could run that script; without it,
    the warning that NO_CHECKBASHISMS describes is allowed. `manual` says
    whether the check built the manuals, whose MANUAL_CHECKS must report OK.
    """
    found, counts = findings(log)
    problems = []
    for finding in found:
        if finding.status not in COUNTED:
            continue
        if not checkbashisms and lacks_checkbashisms(finding):
            continue
        patterns = expected.get(finding.check) if finding.status == "NOTE" else None
        if patterns is None:
            problems.append(f"{finding.status}: checking {finding.check}")
            problems += [f"    {line}" for line in finding.lines]
            continue
        for line in finding.lines:
            if not matches([line], patterns):
                problems.append(f"NOTE: checking {finding.check}: {line}")
            elif (size := TARBALL_SIZE.fullmatch(line)) and int(
                size[1]
            ) > TARBALL_LIMIT:
                problems.append(tarball_problem(int(size[1])))
    if manual:
        reported = {finding.check: finding.status for finding in found}
        problems += [
            f"checking {check}: not OK, as a check with the manuals must report"
            for check in MANUAL_CHECKS
            if reported.get(check) != "OK"
        ]
    seen = dict.fromkeys(COUNTED, 0)
    for finding in found:
        if finding.status in seen:
            seen[finding.status] += 1
    if seen != counts:
        problems.append(f"the Status line counts {counts}, the log shows {seen}")
    return problems


def tarball_problem(size):
    return (
        f"the tarball is {size} bytes, above the {TARBALL_LIMIT} that "
        "cran-comments.md requests"
    )


def report(log, checkbashisms=True, manual=False):
    """Print the check's findings; fail on any that EXPECTED_NOTES lacks, and
    with `manual` unless the manuals were built and validated."""
    found = findings(log)[0]
    for finding in found:
        if finding.status in (*COUNTED, "INFO"):
            print(f"{finding.status}: checking {finding.check}")
            for line in finding.lines:
                print(f"    {line}")
    problems = unexpected(log, checkbashisms=checkbashisms, manual=manual)
    if problems:
        raise RuntimeError(
            "R CMD check --as-cran found what R/yamaa/cran-comments.md does not "
            "explain:\n" + "\n".join(problems)
        )
    if any(map(lacks_checkbashisms, found)):
        print(
            "checkbashisms is not installed, so R did not check configure and cleanup"
        )
    if manual:
        print("R CMD check built the PDF manual and validated the HTML manual")
    print("R CMD check --as-cran: every finding is one that cran-comments.md explains")


def r_config(name, environment):
    """One variable of the R installation's own build configuration."""
    return subprocess.run(
        ["R", "CMD", "config", name],
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def check_environment(environment=None):
    """The environment of a check that reports what CRAN would see.

    It adds CRAN_SETTINGS. Remote services stay out of it unless the caller
    sets these: CRAN's package index and the URL checks
    (`_R_CHECK_CRAN_INCOMING_REMOTE_`), and the clock check
    (`_R_CHECK_SYSTEM_CLOCK_`). What the checking machine contributes stays
    out too: the flags of R's own build configuration are known, as on CRAN's
    check machines, so "checking compilation flags used" reports only flags
    that the package adds, and OpenBLAS runs one thread.
    """
    environment = dict(os.environ if environment is None else environment)
    for name, value in CRAN_SETTINGS.items():
        environment.setdefault(name, value)
    if shutil.which("aspell", path=environment.get("PATH")):
        environment.setdefault("_R_CHECK_CRAN_INCOMING_USE_ASPELL_", "true")
    environment.setdefault("_R_CHECK_CRAN_INCOMING_REMOTE_", "false")
    environment.setdefault("_R_CHECK_SYSTEM_CLOCK_", "false")
    # The machine's BLAS is not the package's, which calls none: the OpenBLAS
    # that setup-r installs on Linux runners keeps its threads busy while each
    # R process starts, and the CPU limits would count them, notably for a
    # test that starts a second R process.
    environment.setdefault("OPENBLAS_NUM_THREADS", "1")
    known = [environment.get("_R_CHECK_COMPILATION_FLAGS_KNOWN_", "")]
    for name in ("CC", "CPPFLAGS", "CPICFLAGS", "CFLAGS", "LDFLAGS"):
        known.append(r_config(name, environment))
    environment["_R_CHECK_COMPILATION_FLAGS_KNOWN_"] = " ".join(
        dict.fromkeys(" ".join(known).split())
    )
    return environment


def has_checkbashisms(environment=None):
    environment = os.environ if environment is None else environment
    return shutil.which("checkbashisms", path=environment.get("PATH")) is not None


def print_output(path):
    """Print one output file of a check whole, between two marker lines."""
    print(f"===== {path.name} =====")
    print(path.read_text(encoding="utf-8", errors="replace"), end="")
    print(f"===== end of {path.name} =====")


def print_failed_tests(work):
    """Print the complete output of each test file that failed in a check.

    R prints only the last lines of a failing file, and the caller may remove
    the check directory, so each `*.Rout.fail` is printed whole while it is
    still there. Returns the names of the files printed.
    """
    failed = sorted(work.glob("*.Rcheck/tests/*.Rout.fail"))
    for path in failed:
        print_output(path)
    return [path.name for path in failed]


def sanitizer_reports(work):
    """The outputs of a check in `work` that hold a sanitizer report, in the
    order of SANITIZER_OUTPUTS."""
    return [
        path
        for pattern in SANITIZER_OUTPUTS
        for path in sorted(work.glob(f"*.Rcheck/{pattern}"))
        if SANITIZER_REPORT.search(path.read_text(encoding="utf-8", errors="replace"))
    ]


def check(archive, work, environment=None, manual=False, ignore_vignettes=False):
    archive, work = archive.resolve(), work.resolve()
    if work.is_relative_to(ROOT):
        raise ValueError("R package checks must run outside the checkout")
    if not archive.is_file():
        raise ValueError("archive must identify the built source archive")
    if archive.stat().st_size > TARBALL_LIMIT:
        raise RuntimeError(tarball_problem(archive.stat().st_size))
    work.mkdir(parents=True)
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    )
    (work / "source-revision.txt").write_text(revision, encoding="utf-8")
    environment = check_environment(environment)
    # Every test file runs, so one failing file does not hide the others.
    # Without `manual`, R leaves out the PDF and HTML manuals.
    command = [
        "R",
        "CMD",
        "check",
        "--as-cran",
        *([] if manual else ["--no-manual"]),
        *(["--ignore-vignettes"] if ignore_vignettes else []),
        "--no-stop-on-test-error",
        str(archive),
    ]
    completed = subprocess.run(command, cwd=work, env=environment, check=False)
    printed = print_failed_tests(work)
    # Each output with a report is printed whole, once, for the stack that
    # the report gives.
    reports = sanitizer_reports(work)
    for path in reports:
        if path.name not in printed:
            print_output(path)
    if completed.returncode != 0:
        raise subprocess.CalledProcessError(completed.returncode, command)
    if reports:
        raise RuntimeError(
            "these outputs of the check hold a sanitizer report: "
            + ", ".join(str(path.relative_to(work)) for path in reports)
        )
    logs = list(work.glob("*.Rcheck/00check.log"))
    if len(logs) != 1:
        raise RuntimeError("R package check wrote no single 00check.log")
    report(
        logs[0].read_text(encoding="utf-8"),
        checkbashisms=has_checkbashisms(environment),
        manual=manual,
    )


def main():
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n\n")[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--log", type=Path, help="compare an existing 00check.log")
    parser.add_argument(
        "--manual",
        action="store_true",
        help="build and validate the PDF and HTML manuals, which needs LaTeX and "
        "HTML Tidy, and require both checks to report OK",
    )
    parser.add_argument(
        "--ignore-vignettes",
        action="store_true",
        help="omit vignette checks when the dependency-only lane has no builders",
    )
    args = parser.parse_args()
    if args.log is not None:
        if args.archive is not None or args.work_dir is not None:
            parser.error("--log compares a log and takes no --archive or --work-dir")
        report(
            args.log.read_text(encoding="utf-8"),
            checkbashisms=has_checkbashisms(),
            manual=args.manual,
        )
        return
    if args.archive is None or args.work_dir is None:
        parser.error("--archive and --work-dir are required without --log")
    archive, work = args.archive.resolve(), args.work_dir.resolve()
    if work.is_relative_to(ROOT):
        parser.error("R package checks must run outside the checkout")
    if not archive.is_file():
        parser.error("--archive must identify the built source archive")
    if not os.environ.get("R_LIBS"):
        parser.error(
            "R_LIBS must identify the installed package and dependency library"
        )
    check(archive, work, manual=args.manual, ignore_vignettes=args.ignore_vignettes)


if __name__ == "__main__":
    main()
