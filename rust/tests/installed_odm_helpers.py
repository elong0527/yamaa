"""Retained public XML/Parquet IO, independently qualified outside the checkout."""

import importlib.abc
import importlib.util
import tempfile
from pathlib import Path

import polars as pl
import yamaa


class RefuseReference(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        forbidden = (
            "yamaa._reference_domain",
            "yamaa.application",
            "yamaa.adapters",
            "yamaa.expressions",
            "yamaa.io",
            "yamaa.models",
            "yamaa.planning",
            "yamaa.runtime",
            "yamaa.schema",
            "yamaa.specification",
            "yamaa.submission",
            "yamaa.verification",
            "yamaa.odm",
        )
        if any(
            fullname == name or fullname.startswith(name + ".") for name in forbidden
        ):
            raise AssertionError("retained helper imported reference: " + fullname)


def main():
    import sys

    sys.meta_path.insert(0, RefuseReference())
    spec = importlib.util.spec_from_file_location(
        "authored_odm", Path("odm_documents.py")
    )
    documents = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(documents)
    with tempfile.TemporaryDirectory(prefix="yamaa-retained-odm-") as directory:
        root = Path(directory)
        for label, xml, values, present, nulls in (
            (
                "1.3",
                documents.ODM_13,
                ["2026-01-02", "", None],
                [True, True, False],
                [False, False, True],
            ),
            ("2.0", documents.ODM_20, ["PD-L1", ""], [True, True], [False, False]),
        ):
            source = root / (label + ".xml")
            source.write_text(xml, encoding="utf-8")
            frame = yamaa.read_odm(source)
            assert frame["Value"].to_list() == values
            assert frame["ValuePresent"].to_list() == present
            assert frame["IsNull"].to_list() == nulls
            assert frame["SourceOrdinal"].to_list() == list(range(1, len(values) + 1))
            output = root / (label + ".parquet")
            result = yamaa.write_odm_parquet(source, output, batch_size=1)
            assert result.row_count == len(values) and result.output_path == output
            restored = pl.read_parquet(output)
            assert restored.equals(frame)
            assert (
                pl.read_parquet_metadata(output)["yamaa.schema"]
                == "odm-clinical-item/1"
            )
            before = output.read_bytes()
            try:
                yamaa.write_odm_parquet(source, output)
            except FileExistsError:
                pass
            else:
                raise AssertionError(
                    "existing output was overwritten without permission"
                )
            assert output.read_bytes() == before
    print(
        "retained public ODM1.3/2.0 values, fixed schema and atomic publication passed without reference imports"
    )


if __name__ == "__main__":
    main()
