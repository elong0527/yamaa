"""Supported platforms in the release notes claims what the native matrix qualifies."""

import contextlib
import io
import unittest
from unittest import mock

from tools.ci import manylinux, native
from tools.ci import select_release_artifacts as release
from tools.commands import repository
from tools.validation import check_supported_platforms as platforms

PAGE = f"{platforms.LABEL}: Supported platforms"
LINUX, MACOS = "Linux x86_64", "macOS arm64"
WHEEL, SOURCE = platforms.WHEEL, platforms.PYTHON_SOURCE
R_SOURCE, R_BINARY, CLI = platforms.R_SOURCE, platforms.R_BINARY, platforms.CLI

# An invented table of two platforms.
TABLE = (
    "| Platform | Python wheel | Python source archive | R source package "
    "| R binary package | Command-line binary |",
    "| --- | --- | --- | --- | --- | --- |",
    "| Linux x86_64 | CPython 3.12 to 3.13 on glibc 2.28 or later | CPython 3.12 "
    "| R 4.4 or later | no | `x86_64-unknown-linux-musl` |",
    "| macOS arm64 | CPython 3.12 and 3.13 | CPython 3.12 | R 4.6 | R 4.5 and 4.6 "
    "| `aarch64-apple-darwin` |",
)


def lane(job, platform, form, kind, value=None):
    return platforms.Lane(job, platform, form, kind, value)


# The invented lanes that qualify exactly what TABLE claims.
LANES = (
    lane("qualify (linux, 3.12, wheel)", LINUX, WHEEL, "series", "3.12"),
    lane("qualify (linux, 3.13, wheel)", LINUX, WHEEL, "series", "3.13"),
    lane("glibc (linux)", LINUX, WHEEL, "glibc", "2.28"),
    lane("qualify (linux, 3.12, source)", LINUX, SOURCE, "series", "3.12"),
    lane("offline (linux, python)", LINUX, SOURCE, "form"),
    lane("cran (linux, R 4.4)", LINUX, R_SOURCE, "series", "4.4"),
    lane("r (linux)", LINUX, R_SOURCE, "series", "4.6"),
    lane("cli-install (linux)", LINUX, CLI, "target", "x86_64-unknown-linux-musl"),
    lane("qualify (macos, 3.12, wheel)", MACOS, WHEEL, "series", "3.12"),
    lane("qualify (macos, 3.13, wheel)", MACOS, WHEEL, "series", "3.13"),
    lane("qualify (macos, 3.12, source)", MACOS, SOURCE, "series", "3.12"),
    lane("r (macos)", MACOS, R_SOURCE, "series", "4.6"),
    lane("r-binary-install (macos, R 4.5.3)", MACOS, R_BINARY, "series", "4.5"),
    lane("r-binary-install (macos, R 4.6.1)", MACOS, R_BINARY, "series", "4.6"),
    lane("cli-install (macos)", MACOS, CLI, "target", "aarch64-apple-darwin"),
)
NEWEST = {"CPython": "3.13", "R": "4.6"}


def page(*table):
    """Invented release notes whose Supported platforms holds `table` beside
    another table, followed by a section whose own table starts with Platform."""
    return "\n".join(
        (
            "# yamaa 0.7.0",
            "",
            "## Supported platforms",
            "",
            "Each row is a platform.",
            "",
            *table,
            "",
            "| Release | Python |",
            "| --- | --- |",
            "| Final release | `X.Y.Z` |",
            "",
            "## Saved artifact readers",
            "",
            "| Platform | Reader |",
            "| --- | --- |",
            "| Linux x86_64 | Polars |",
            "",
        )
    )


def edited(platform, column, text, table=TABLE):
    """`table` with the cell of `platform` under `column` replaced by `text`."""
    index = platforms.HEADER.index(column)
    rows = []
    for line in table:
        cells = platforms.cells(line)
        if cells[0] == platform:
            cells[index] = text
            line = "| " + " | ".join(cells) + " |"
        rows.append(line)
    return rows


def check(table=TABLE, found=LANES, newest=NEWEST):
    return platforms.problems(platforms.read(page(*table)), list(found), newest)


class ProblemsTest(unittest.TestCase):
    def test_a_table_that_claims_what_the_lanes_qualify_passes(self):
        self.assertEqual(check(), [])

    def test_a_series_that_no_lane_qualifies_fails(self):
        self.assertEqual(
            check(edited(LINUX, WHEEL, "CPython 3.11 to 3.13 on glibc 2.28 or later")),
            [
                f"{PAGE} claims CPython 3.11 for the Python wheel on Linux x86_64, "
                "and no native lane qualifies it"
            ],
        )
        self.assertEqual(
            check(edited(MACOS, R_SOURCE, "R 4.4 and 4.6")),
            [
                f"{PAGE} claims R 4.4 for the R source package on macOS arm64, and no "
                "native lane qualifies it"
            ],
        )

    def test_a_floor_needs_itself_and_the_newest_series_qualified(self):
        self.assertEqual(
            check(edited(MACOS, R_SOURCE, "R 4.4 or later")),
            [
                f"{PAGE} claims R 4.4 or later for the R source package on macOS "
                "arm64, and no native lane qualifies R 4.4 there"
            ],
        )
        without_release = [item for item in LANES if item.job != "r (linux)"]
        self.assertEqual(
            check(found=without_release),
            [
                f"{PAGE} claims R 4.4 or later for the R source package on Linux "
                "x86_64, and no native lane qualifies R 4.6 there"
            ],
        )
        # A series between the floor and the newest needs no lane, and one
        # that a lane qualifies is claimed.
        between = lane("cran (linux, R 4.5)", LINUX, R_SOURCE, "series", "4.5")
        self.assertEqual(check(found=(*LANES, between)), [])

    def test_a_form_that_no_lane_qualifies_fails(self):
        self.assertEqual(
            check(edited(LINUX, R_BINARY, "R 4.6")),
            [
                f"{PAGE} claims the R binary package on Linux x86_64, and no native "
                "lane qualifies it"
            ],
        )

    def test_a_platform_that_no_lane_qualifies_fails(self):
        row = "| macOS x86_64 | CPython 3.12 | no | no | no | `x86_64-apple-darwin` |"
        self.assertEqual(
            check((*TABLE, row)),
            [
                f"{PAGE} claims the Python wheel on macOS x86_64, and no native lane "
                "qualifies it",
                f"{PAGE} claims the command-line binary on macOS x86_64, and no "
                "native lane qualifies it",
            ],
        )

    def test_a_lane_that_proves_the_form_alone_proves_no_series(self):
        offline_only = [
            item for item in LANES if item.job != "qualify (linux, 3.12, source)"
        ]
        self.assertEqual(
            check(found=offline_only),
            [
                f"{PAGE} claims CPython 3.12 for the Python source archive on Linux "
                "x86_64, and no native lane qualifies it"
            ],
        )

    def test_the_glibc_that_a_wheel_cell_names_is_the_one_its_lane_qualifies(self):
        self.assertEqual(
            check(edited(MACOS, WHEEL, "CPython 3.12 and 3.13 on glibc 2.28 or later")),
            [
                f"{PAGE} claims glibc 2.28 or later for the Python wheel on macOS "
                "arm64, and no native lane qualifies it"
            ],
        )
        self.assertEqual(
            check(edited(LINUX, WHEEL, "CPython 3.12 to 3.13")),
            [
                f"{PAGE} does not claim glibc 2.28 or later for the Python wheel on "
                "Linux x86_64, which glibc (linux) qualifies"
            ],
        )
        self.assertEqual(
            check(edited(LINUX, WHEEL, "CPython 3.12 to 3.13 on glibc 2.17 or later")),
            [
                f"{PAGE} claims glibc 2.17 or later for the Python wheel on Linux "
                "x86_64, and no native lane qualifies it",
                f"{PAGE} does not claim glibc 2.28 or later for the Python wheel on "
                "Linux x86_64, which glibc (linux) qualifies",
            ],
        )

    def test_the_command_line_cell_names_the_target_that_its_lane_builds(self):
        self.assertEqual(
            check(edited(LINUX, CLI, "`x86_64-unknown-linux-gnu`")),
            [
                f"{PAGE} names `x86_64-unknown-linux-gnu` for the command-line binary "
                "on Linux x86_64, and no native lane qualifies it",
                f"{PAGE} does not name `x86_64-unknown-linux-musl` for the "
                "command-line binary on Linux x86_64, which cli-install (linux) "
                "qualifies",
            ],
        )

    def test_a_lane_on_a_platform_without_a_row_fails(self):
        arm = (
            lane(
                "qualify (arm, 3.12, wheel)", "Linux aarch64", WHEEL, "series", "3.12"
            ),
            lane("cli-install (arm)", "Linux aarch64", CLI, "target", "aarch64-musl"),
        )
        self.assertEqual(
            check(found=(*LANES, *arm)),
            [
                f"{PAGE} has no row for Linux aarch64, where native lanes qualify the "
                "Python wheel and command-line binary"
            ],
        )

    def test_a_series_that_a_lane_qualifies_and_no_cell_claims_fails(self):
        newer = (
            lane("qualify (linux, 3.14, wheel)", LINUX, WHEEL, "series", "3.14"),
            lane("install (linux, 3.14, wheel)", LINUX, WHEEL, "series", "3.14"),
        )
        self.assertEqual(
            check(found=(*LANES, *newer)),
            [
                f"{PAGE} does not claim CPython 3.14 for the Python wheel on Linux "
                "x86_64, which install (linux, 3.14, wheel) and qualify (linux, "
                "3.14, wheel) qualify"
            ],
        )
        # Below a floor.
        older = lane("cran (linux, R 4.3)", LINUX, R_SOURCE, "series", "4.3")
        self.assertEqual(
            check(found=(*LANES, older)),
            [
                f"{PAGE} does not claim R 4.3 for the R source package on Linux "
                "x86_64, which cran (linux, R 4.3) qualifies"
            ],
        )

    def test_a_cell_that_says_no_where_a_lane_qualifies_fails(self):
        self.assertEqual(
            check(edited(MACOS, R_BINARY, "no")),
            [
                f"{PAGE} says no for the R binary package on macOS arm64, which "
                "r-binary-install (macos, R 4.5.3) and r-binary-install (macos, "
                "R 4.6.1) qualify"
            ],
        )
        self.assertEqual(
            check(edited(LINUX, SOURCE, "no")),
            [
                f"{PAGE} says no for the Python source archive on Linux x86_64, "
                "which offline (linux, python) and qualify (linux, 3.12, source) "
                "qualify"
            ],
        )

    def test_a_cell_not_written_as_its_column_admits_fails(self):
        for column, text, reason in (
            (WHEEL, "Python 3.12", "expected CPython and its series, or no"),
            (WHEEL, "CPython 3.12-3.13", "'3.12-3.13' is neither a series nor a range"),
            (WHEEL, "CPython 3.13 to 3.12", "does not run from an older to a newer"),
            (WHEEL, "CPython 3.12 to 4.1", "does not run from an older to a newer"),
            (WHEEL, "CPython 3.12, 3.12 to 3.13", "it names a series twice"),
            # Only the R source package states a floor, and only the wheel a glibc.
            (WHEEL, "CPython 3.12 or later", "'3.12 or later' is neither"),
            (R_BINARY, "R 4.5 or later", "'4.5 or later' is neither"),
            (SOURCE, "CPython 3.12 on glibc 2.28 or later", "is neither a series"),
            (R_SOURCE, "R 4.4 or later on glibc 2.28 or later", "is neither"),
            (R_SOURCE, "", "expected R and its series, or no"),
            (CLI, "x86_64-unknown-linux-musl", "expected a Rust target in backquotes"),
            (CLI, "yes", "expected a Rust target in backquotes"),
        ):
            with self.subTest(column=column, text=text):
                messages = check(edited(LINUX, column, text))
                # An unreadable cell is reported once, and nothing is compared
                # with its lanes.
                self.assertEqual(len(messages), 1, messages)
                noun = platforms.COLUMNS[column].noun
                self.assertTrue(
                    messages[0].startswith(
                        f"{PAGE} cannot read the {noun} cell of Linux x86_64, "
                        f"{text!r}: "
                    ),
                    messages[0],
                )
                self.assertIn(reason, messages[0])

    def test_cells_read_as_the_claims_they_write(self):
        Claim = platforms.Claim
        for column, text, expected in (
            (
                WHEEL,
                "CPython 3.12 to 3.15 on glibc 2.28 or later",
                Claim(frozenset({"3.12", "3.13", "3.14", "3.15"}), glibc="2.28"),
            ),
            (
                SOURCE,
                "CPython 3.12, 3.13 and 3.15",
                Claim(frozenset({"3.12", "3.13", "3.15"})),
            ),
            (R_SOURCE, "R 4.4 or later", Claim(floor="4.4")),
            (R_SOURCE, "R 4.6", Claim(frozenset({"4.6"}))),
            (R_BINARY, "R 4.5 and 4.6", Claim(frozenset({"4.5", "4.6"}))),
            (CLI, "`aarch64-apple-darwin`", Claim(target="aarch64-apple-darwin")),
            (R_BINARY, "no", None),
        ):
            with self.subTest(column=column, text=text):
                self.assertEqual(platforms.claim(column, text), expected)
        floor = Claim(floor="4.4")
        self.assertEqual(
            [floor.covers(series) for series in ("4.3", "4.4", "4.10")],
            [False, True, True],
        )


class ReadTest(unittest.TestCase):
    def test_the_table_is_read_from_its_section_alone(self):
        rows = platforms.read(page(*TABLE))
        self.assertEqual(list(rows), [LINUX, MACOS])
        self.assertEqual(
            rows[MACOS],
            {
                WHEEL: "CPython 3.12 and 3.13",
                SOURCE: "CPython 3.12",
                R_SOURCE: "R 4.6",
                R_BINARY: "R 4.5 and 4.6",
                CLI: "`aarch64-apple-darwin`",
            },
        )

    def test_a_page_without_one_section_or_one_table_refuses(self):
        heading = "expected one '## Supported platforms' heading"
        table = "expected one table under Supported platforms whose first heading"
        for text, message in (
            ("# yamaa 0.7.0\n", heading),
            (page(*TABLE) + "## Supported platforms\n", heading),
            (page(), table),
            (page(*TABLE, "", *TABLE), table),
        ):
            with (
                self.subTest(message=message),
                self.assertRaisesRegex(ValueError, message),
            ):
                platforms.read(text)

    def test_a_malformed_table_refuses(self):
        header, rule, linux, macos = TABLE
        short_rule = "| --- | --- | --- | --- | --- |"
        short_row = "| Linux x86_64 | CPython 3.12 | CPython 3.12 | R 4.6 | no |"
        for table, message in (
            (
                (header.replace("Command-line binary", "Command line"), rule, linux),
                "headings are Platform \\| Python wheel .* \\| Command line, not "
                "Platform \\| Python wheel",
            ),
            ((header, linux, macos), "no delimiter row"),
            ((header, short_rule, linux), "no delimiter row"),
            ((header,), "no delimiter row"),
            ((header, rule, short_row), "has 5 cells, not 6"),
            ((header, rule, linux + " extra |"), "has 7 cells, not 6"),
            ((header, rule, linux, macos, linux), "two rows for Linux x86_64"),
            ((header, rule), "has no rows"),
        ):
            with (
                self.subTest(message=message),
                self.assertRaisesRegex(ValueError, message),
            ):
                platforms.read(page(*table))


class LanesTest(unittest.TestCase):
    def test_every_kind_of_native_lane_is_qualifying_or_diagnostic(self):
        self.assertFalse(platforms.QUALIFYING & platforms.DIAGNOSTIC)
        self.assertEqual(
            set(native.coverage()), platforms.QUALIFYING | platforms.DIAGNOSTIC
        )

    def test_a_kind_of_lane_that_is_not_classified_refuses(self):
        coverage = dict(native.coverage(), musl_wheel={"os": ["ubuntu-24.04"]})
        with self.assertRaisesRegex(
            ValueError,
            "classifies neither as qualifying nor as diagnostic: musl_wheel",
        ):
            platforms.lanes(coverage)

    def test_a_runner_without_a_platform_name_refuses(self):
        coverage = native.coverage()
        coverage["cli"]["include"].append(
            {"os": "macos-15-intel", "target": "x86_64-apple-darwin"}
        )
        with self.assertRaisesRegex(
            ValueError,
            "native-cli-install \\(macos-15-intel, x86_64-apple-darwin\\): .* names "
            "no platform for macos-15-intel",
        ):
            platforms.lanes(coverage)

    def test_a_python_form_or_offline_host_that_is_not_known_refuses(self):
        coverage = native.coverage()
        coverage["qualify"]["include"].append(
            {"os": "ubuntu-24.04", "python": "3.12", "form": "editable"}
        )
        with self.assertRaisesRegex(
            ValueError,
            "native-qualify \\(ubuntu-24.04, 3.12, editable\\): .* does not know the "
            "Python form editable",
        ):
            platforms.lanes(coverage)
        coverage = native.coverage()
        coverage["offline_source"]["host"].append("cli")
        with self.assertRaisesRegex(
            ValueError,
            "native-offline-source \\(ubuntu-24.04, cli\\): .* does not know the "
            "host cli",
        ):
            platforms.lanes(coverage)

    def test_each_lane_is_a_job_that_a_release_requires(self):
        jobs = release.required_jobs(native.coverage())
        for item in platforms.lanes():
            with self.subTest(job=item.job):
                self.assertIn(item.job, jobs)

    def test_diagnostic_lanes_and_r_devel_qualify_nothing(self):
        jobs = {item.job for item in platforms.lanes()}
        for job in release.required_jobs(native.coverage()):
            diagnostic = job.startswith(
                ("native-rust ", "native-dist ", "native-share ")
            ) or (
                job.startswith("native-cran ") and ("R devel" in job or "R " not in job)
            )
            if diagnostic:
                with self.subTest(job=job):
                    self.assertNotIn(job, jobs)
        self.assertIn(f"native-cran ({native.PLATFORMS[0]}, R 4.2.0)", jobs)

    def test_each_r_lane_proves_the_series_of_its_r(self):
        release_series = native.r_series(native.R_RELEASE)
        self.assertEqual(
            [platforms.cran_series(r) for r in ("devel", "release", "oldrel")],
            [None, release_series, native.r_series(native.R_OLDREL)],
        )
        self.assertEqual(platforms.cran_series("4.4"), "4.4")
        self.assertEqual(platforms.cran_series("4.4.3"), "4.4")
        with self.assertRaisesRegex(ValueError, "R series of native-cran's R next"):
            platforms.cran_series("next")
        # The R of the other R source lanes is R-release, and the offline
        # Python lanes, whose CPython native.py does not declare, prove their
        # form alone.
        offline = ("native-offline-source", "native-offline-toolchain")
        for item in platforms.lanes():
            name = item.job.partition(" (")[0]
            with self.subTest(job=item.job):
                if item.form == R_SOURCE and name in ("native-r", *offline):
                    self.assertEqual(
                        (item.kind, item.value), ("series", release_series)
                    )
                if item.form == SOURCE and name in offline:
                    self.assertEqual((item.kind, item.value), ("form", None))

    def test_each_offline_wheel_lane_proves_its_wheel_with_its_cpython(self):
        # The lanes that install each wheel with no network declare their
        # CPython, so each proves that series of its platform's wheel.
        coverage = native.coverage()["offline_wheel"]
        self.assertEqual(
            [
                item
                for item in platforms.lanes()
                if item.job.startswith("native-offline-wheel ")
            ],
            [
                lane(
                    f"native-offline-wheel ({runner}, {python})",
                    platforms.PLATFORMS[runner],
                    WHEEL,
                    "series",
                    python,
                )
                for runner in coverage["os"]
                for python in coverage["python"]
            ],
        )

    def test_the_oldest_glibc_is_the_one_the_manylinux_policy_admits(self):
        self.assertEqual(platforms.oldest_glibc(), "2.28")
        with mock.patch.object(manylinux, "POLICY", "manylinux_2_34"):
            self.assertEqual(platforms.oldest_glibc(), "2.34")
        with (
            mock.patch.object(manylinux, "POLICY", "manylinux2014"),
            self.assertRaisesRegex(ValueError, "cannot read a glibc version"),
        ):
            platforms.oldest_glibc()


class RealFilesTest(unittest.TestCase):
    def setUp(self):
        self.rows = platforms.read(platforms.PAGE.read_text(encoding="utf-8"))

    def test_the_page_claims_exactly_what_the_native_matrix_qualifies(self):
        self.assertEqual(
            platforms.problems(self.rows, platforms.lanes(), platforms.newest()), []
        )

    def test_check_repository_runs_the_check(self):
        self.assertIn("tools.validation.check_supported_platforms", repository.CHECKS)

    def test_main_passes_on_the_real_files(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            platforms.main()
        self.assertEqual(
            output.getvalue(),
            f"{platforms.LABEL}: Supported platforms claims what "
            f"{len(platforms.lanes())} qualifying native lanes prove on "
            f"{len(self.rows)} platforms, and nothing else\n",
        )

    def test_main_names_each_gap_and_how_to_close_it(self):
        coverage = native.coverage()
        coverage["qualify"]["include"].append(
            {"os": "ubuntu-24.04", "python": "3.16", "form": "wheel"}
        )
        errors = io.StringIO()
        with (
            mock.patch.object(
                platforms, "lanes", return_value=platforms.lanes(coverage)
            ),
            contextlib.redirect_stderr(errors),
            self.assertRaises(SystemExit) as stopped,
        ):
            platforms.main()
        self.assertEqual(stopped.exception.code, 1)
        self.assertEqual(
            errors.getvalue().splitlines(),
            [
                f"{PAGE} does not claim CPython 3.16 for the Python wheel on Linux "
                "x86_64, which native-qualify (ubuntu-24.04, 3.16, wheel) qualifies",
                platforms.HINT,
            ],
        )

    def test_r_4_2_on_linux_aarch64_needs_a_lane(self):
        rows = dict(self.rows)
        rows["Linux aarch64"] = dict(rows["Linux aarch64"])
        rows["Linux aarch64"][R_SOURCE] = "R 4.2 or later"
        self.assertEqual(
            platforms.problems(rows, platforms.lanes(), platforms.newest()),
            [
                f"{PAGE} claims R 4.2 or later for the R source package on Linux "
                "aarch64, and no native lane qualifies R 4.2 there"
            ],
        )
        # And the R 4.2.0 lane that would prove it needs the claim.
        coverage = native.coverage()
        coverage["cran"]["os"].append("ubuntu-24.04-arm")
        self.assertEqual(
            platforms.problems(
                self.rows, platforms.lanes(coverage), platforms.newest()
            ),
            [
                f"{PAGE} does not claim R 4.2 for the R source package on Linux "
                "aarch64, which native-cran (ubuntu-24.04-arm, R 4.2.0) qualifies"
            ],
        )


if __name__ == "__main__":
    unittest.main()
