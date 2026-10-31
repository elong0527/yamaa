"""The performance harness's own seams (tools/performance): the generator
gives the same tables at a size every time, with the geometry its docstring
states, and the equality check that gates a comparison's times accepts
polars' bds output at the smallest size and refuses a changed value."""

import tempfile
import unittest
from pathlib import Path

import polars as pl
import yamaa

from tools.performance import generate, run
from tools.performance.polars_cases import derive


class Generator(unittest.TestCase):
    def test_a_size_gives_the_same_tables_every_time(self):
        for name, build in (
            ("adsl", generate.adsl),
            ("lb", generate.lb),
            ("vitals", generate.vitals),
        ):
            with self.subTest(case=name):
                self.assertTrue(build(3).equals(build(3)), name)

    def test_lb_has_72_records_per_subject_and_a_value_on_every_day_minus_one(self):
        # 6 tests x 12 visits per subject; day -1 is the baseline visit, never
        # missing, so every subject and test has a baseline record.
        frame = generate.lb(40)
        day = (pl.col("LBDTC") - generate.FIRST_TREATMENT).dt.total_days()
        start = (pl.col("USUBJID").str.slice(-6).cast(pl.Int64) % 28).cast(pl.Int64)
        relative = frame.select((day - start).alias("day"), "LBSTRESN")
        self.assertEqual(
            (
                frame.height,
                sorted(relative["day"].unique().to_list()),
                relative.filter(pl.col("day") == -1)["LBSTRESN"].null_count(),
            ),
            (40 * 72, sorted(generate.DAYS), 0),
        )


class Equality(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-performance-")
        cls.addClassCleanup(directory.cleanup)
        work = Path(directory.name)
        cls.case = run._cases()["bds"]
        data = run.inputs(cls.case, 18_000, work)
        cls.root = work / "bds"
        run.stage(cls.case, cls.case.engines[0], data, cls.root)
        yamaa.spec(cls.root / "spec.yaml").run().save()
        cls.reference = cls.root / "adlb.parquet"
        cls.polars = derive(
            pl.scan_parquet(cls.root / "input" / "lb.parquet"),
            pl.scan_parquet(cls.root / "input" / "adsl.parquet"),
        ).collect()

    def test_polars_bds_output_equals_yamaa_output_sorted_by_key(self):
        candidate = self.root / "polars.parquet"
        # Another row order is no difference: the check sorts by the keys.
        self.polars.reverse().write_parquet(candidate)
        self.assertIsNone(run.difference(self.case, self.reference, candidate))

    def test_a_changed_value_is_a_difference(self):
        candidate = self.root / "changed.parquet"
        self.polars.with_columns(
            pl.when(pl.int_range(pl.len()) == 0)
            .then(pl.col("AVAL") + 1)
            .otherwise(pl.col("AVAL"))
            .alias("AVAL")
        ).write_parquet(candidate)
        self.assertIsNotNone(run.difference(self.case, self.reference, candidate))


if __name__ == "__main__":
    unittest.main()
