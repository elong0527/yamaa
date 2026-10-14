"""Authored shared grammar decisions through the installed Python domain facade."""

import csv
import runpy
from pathlib import Path

import yamaa

TRUTH = runpy.run_path(str(Path(__file__).with_name("host-fixtures") / "reader.py"))["load"](
    __file__, "public-grammar"
)

GRAMMAR = {name: set(values) for name, values in TRUTH["grammar_conditions"].items()}


def main():
    root = Path("grammar-original")
    with (root / "cases.tsv").open(encoding="utf-8", newline="") as stream:
        vectors = list(csv.DictReader(stream, delimiter="\t"))
    for case in vectors:
        issues = yamaa.domain(root / case["specification"]).issues
        conditions = [row["condition"] for row in issues.to_dicts()]
        observed = (
            conditions
            if case["parse"] == "reject"
            else [
                condition
                for condition in conditions
                if condition in GRAMMAR[case["contract"]]
            ]
        )
        expected = [case["public_condition"]] if case["parse"] == "reject" else []
        assert observed == expected, (case, issues)
        assert not (root / "unwritten.csv").exists()
    print(f"{len(vectors)} installed Python public shared grammar checks passed")


if __name__ == "__main__":
    main()
