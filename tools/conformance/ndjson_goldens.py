"""Write the newline-delimited Dataset-JSON goldens from the authored
single-object goldens.

A benchmark whose section sets ``transport: dataset_ndjson`` expects the same
dataset as the ``dataset_json`` golden it is authored beside (JSON-4): the
first line is that object without ``rows``, in the one-line spelling JSON-13
fixes, and each later line is one row. This tool derives those bytes from the
authored ``.json`` golden alone, never from the engine's writer, so the
expected data stays independently authored; the tests re-derive every golden
and compare it with the committed bytes. The goldens' cells are text,
integers and missing values, whose JSON spelling the standard library writes
as JSON-13 fixes it; a float cell is refused rather than spelled.
``python -m tools.conformance.ndjson_goldens`` rewrites the goldens.
"""

import argparse
import json

from tools.paths import ROOT

# Each golden: its benchmark and file, and the authored single-object golden
# it is derived from, as benchmark and file.
GOLDENS = [
    ("schema-transport-dataset-ndjson", "dm.ndjson", "sdtm-dm-metadata", "dm.json"),
]


def text(value):
    """One JSON text in JSON-13's one-line spelling: a comma and one space
    between members and elements, a colon and one space after a name."""
    return json.dumps(value, ensure_ascii=False, separators=(", ", ": "))


def lines(document):
    """The newline-delimited bytes of one single-object document."""
    for row in document["rows"]:
        for cell in row:
            if isinstance(cell, bool) or not isinstance(cell, (str, int, type(None))):
                raise ValueError(
                    f"{cell!r}: only text, integer and missing cells are spelled here"
                )
    head = {key: value for key, value in document.items() if key != "rows"}
    return "".join(text(line) + "\n" for line in [head, *document["rows"]]).encode(
        "utf-8"
    )


def derive(golden):
    """The bytes of one ``GOLDENS`` entry, from its authored source."""
    _, _, source, origin = golden
    return lines(
        json.loads((ROOT / "benchmarks" / source / "expected" / origin).read_bytes())
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args(argv)
    for golden in GOLDENS:
        benchmark, name = golden[:2]
        (ROOT / "benchmarks" / benchmark / "expected" / name).write_bytes(
            derive(golden)
        )


if __name__ == "__main__":
    main()
