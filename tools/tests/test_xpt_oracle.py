"""Controls for the independent SAS Transport version 5 oracle, its goldens and
the benchmark transport inputs that it encodes."""

import datetime
import importlib.util
import struct
import unittest

from tools.conformance import xpt_goldens, xpt_oracle
from tools.conformance.xpt_oracle import Format, Variable
from tools.paths import ROOT
from tools.staging import build_xport_fixtures

CREATED = datetime.datetime(2026, 9, 11, 7, 8, 9)


def member():
    variables = [
        Variable("NAME", "Subject name", True, width=5),
        Variable("VALUE", "Measured value", False, format=Format("", 8, 2)),
        Variable("DAY", "Visit date", False, format=Format("DATE", 9)),
    ]
    rows = [
        [" A", -118.625, xpt_oracle.sas_date(datetime.date(1960, 1, 1))],
        [None, None, xpt_oracle.sas_date(datetime.date(1959, 12, 31))],
    ]
    return variables, rows


class IbmDoubles(unittest.TestCase):
    def test_vectors_are_normalized_exact_and_signed(self):
        # Sign bit, excess-64 hexadecimal exponent, 56-bit fraction in [1/16, 1).
        for value, expected in [
            (1, "4110000000000000"),
            (-1, "C110000000000000"),
            (0.5, "4080000000000000"),
            (100, "4264000000000000"),
            (0.1, "401999999999999A"),
            (-118.625, "C276A00000000000"),
            (3653, "43E4500000000000"),
            (2**53, "4E20000000000000"),
            (2.0**-260, "0010000000000000"),
            (2.0**252 * (1 - 2.0**-53), "7FFFFFFFFFFFFFF8"),
            (0.0, "0000000000000000"),
            (-0.0, "0000000000000000"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(xpt_oracle.ibm(value).hex().upper(), expected, value)
        self.assertEqual(xpt_oracle.ibm(None), b".\0\0\0\0\0\0\0")

    def test_values_without_an_exact_ibm_double_are_refused(self):
        for value in (2.0**252, 2.0**-261, 2**57 + 1):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    xpt_oracle.ibm(value)


class Layout(unittest.TestCase):
    def test_records_follow_the_transport_layout(self):
        variables, rows = member()
        data = xpt_oracle.encode("TEST", "Independent member", CREATED, variables, rows)
        records = [data[i : i + 80] for i in range(0, len(data), 80)]
        self.assertEqual(len(data) % 80, 0)
        self.assertEqual(len(data), xpt_oracle.file_size(variables, len(rows)))
        self.assertEqual(
            records[0],
            b"HEADER RECORD*******LIBRARY HEADER RECORD!!!!!!!" + b"0" * 30 + b"  ",
        )
        self.assertEqual(
            records[1],
            b"SAS     SAS     SASLIB  9.4     yamaa   "
            + b" " * 24
            + b"11SEP26:07:08:09",
        )
        self.assertEqual(records[2], b"11SEP26:07:08:09" + b" " * 64)
        self.assertEqual(
            records[3],
            b"HEADER RECORD*******MEMBER  HEADER RECORD!!!!!!!"
            b"000000000000000001600000000140  ",
        )
        self.assertEqual(records[4][:28], b"HEADER RECORD*******DSCRPTR ")
        self.assertEqual(
            records[5],
            b"SAS     TEST    SASDATA 9.4     yamaa   "
            + b" " * 24
            + b"11SEP26:07:08:09",
        )
        self.assertEqual(
            records[6],
            b"11SEP26:07:08:09"
            + b" " * 16
            + b"Independent member".ljust(40)
            + b" " * 8,
        )
        self.assertEqual(
            records[7],
            b"HEADER RECORD*******NAMESTR HEADER RECORD!!!!!!!"
            b"000000000300000000000000000000  ",
        )
        namestrs = data[640 : 640 + 3 * 140]
        fields = struct.unpack(">hhhh8s40s8shhh2s8shhi52s", namestrs[140:280])
        self.assertEqual(fields[:4], (1, 0, 8, 2))
        self.assertEqual(fields[4], b"VALUE   ")
        self.assertEqual(fields[5], b"Measured value".ljust(40))
        self.assertEqual(fields[6:9], (b" " * 8, 8, 2))
        self.assertEqual(fields[11], b" " * 8)
        self.assertEqual(fields[14], 5)
        day = struct.unpack(">hhhh8s40s8shhh2s8shhi52s", namestrs[280:420])
        self.assertEqual(day[:4], (1, 0, 8, 3))
        self.assertEqual(day[6:9], (b"DATE    ", 9, 0))
        self.assertEqual((day[14], day[15]), (13, bytes(52)))
        # Three NAMESTRs fill 420 bytes, padded with blanks to six records.
        self.assertEqual(data[640 + 420 : 640 + 480], b" " * 60)
        self.assertEqual(data[1120:1148], b"HEADER RECORD*******OBS     ")
        observations = data[1200:]
        self.assertEqual(observations[:5], b" A   ")
        self.assertEqual(observations[5:13].hex().upper(), "C276A00000000000")
        self.assertEqual(observations[13:21], bytes(8))
        self.assertEqual(observations[21:26], b"     ")
        self.assertEqual(observations[26:34], b".\0\0\0\0\0\0\0")
        self.assertEqual(observations[34:42].hex().upper(), "C110000000000000")
        self.assertEqual(observations[42:], b" " * 38)

    def test_sas_dates_count_from_1960_and_headers_use_two_digit_years(self):
        self.assertEqual(xpt_oracle.sas_date(datetime.date(2025, 1, 8)), 23749)
        self.assertEqual(
            xpt_oracle.sas_datetime(datetime.datetime(1959, 12, 31, 23, 59, 59)), -1
        )
        self.assertEqual(
            xpt_oracle.header_time(datetime.datetime(2000, 1, 5, 7, 8, 9)),
            b"05JAN00:07:08:09",
        )

    def test_fields_refuse_text_a_transport_cannot_carry(self):
        for text in ("caf\u00e9", "A\tB", "NINECHARS"):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    xpt_oracle.ascii_field(text, 8)


class Goldens(unittest.TestCase):
    def test_committed_goldens_are_the_authored_tables(self):
        for benchmark, file, name, label, variables, rows in xpt_goldens.GOLDENS:
            with self.subTest(benchmark=benchmark, file=file):
                committed = (
                    ROOT / "benchmarks" / benchmark / "expected" / file
                ).read_bytes()
                self.assertEqual(
                    committed,
                    xpt_goldens.golden(benchmark, name, label, variables, rows),
                    benchmark,
                )

    @unittest.skipUnless(
        importlib.util.find_spec("pyreadstat"), "pyreadstat is not installed"
    )
    def test_goldens_read_back_through_pyreadstat(self):
        import pyreadstat

        for benchmark, file, _, label, variables, rows in xpt_goldens.GOLDENS:
            with self.subTest(benchmark=benchmark, file=file):
                frame, meta = pyreadstat.read_xport(
                    str(ROOT / "benchmarks" / benchmark / "expected" / file)
                )
                self.assertEqual(meta.table_name, label)
                self.assertEqual(list(frame.columns), [v.name for v in variables])
                self.assertEqual(len(frame), len(rows))

    @unittest.skipUnless(importlib.util.find_spec("pandas"), "pandas is not installed")
    def test_goldens_read_back_through_pandas(self):
        import pandas

        for benchmark, file, _, _, variables, rows in xpt_goldens.GOLDENS:
            with self.subTest(benchmark=benchmark, file=file):
                frame = pandas.read_sas(
                    ROOT / "benchmarks" / benchmark / "expected" / file, format="xport"
                )
                expected = pandas.DataFrame(
                    {
                        variable.name: [
                            (row[index] or "").encode("ascii")
                            if variable.character
                            else float(row[index])
                            if row[index] is not None
                            else float("nan")
                            for row in rows
                        ]
                        for index, variable in enumerate(variables)
                    }
                )
                pandas.testing.assert_frame_equal(frame, expected, obj=benchmark)


class BenchmarkInput(unittest.TestCase):
    def test_the_committed_input_is_the_authored_table(self):
        # The benchmarks read them through the SAS reader, so their bytes come
        # from the literal tables and the public layout, never from a reader.
        fixtures = build_xport_fixtures
        for path, encoded, variables, rows in [
            (
                fixtures.BENCHMARK_INPUT,
                fixtures.benchmark_input(),
                fixtures.BENCHMARK_VARIABLES,
                fixtures.BENCHMARK_ROWS,
            ),
            (
                fixtures.NEGATIVE_INPUT,
                fixtures.negative_input(),
                fixtures.NEGATIVE_VARIABLES,
                fixtures.NEGATIVE_ROWS,
            ),
        ]:
            with self.subTest(case=path.parent.parent.name):
                self.assertEqual(path.read_bytes(), encoded)
                self.assertEqual(
                    len(encoded), xpt_oracle.file_size(variables, len(rows))
                )


if __name__ == "__main__":
    unittest.main()
