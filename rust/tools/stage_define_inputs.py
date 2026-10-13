"""Copy original shared Define metadata and independent fixed-byte witnesses."""
import argparse
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CASES = ("sdtm-dm-metadata", "sdtm-ae-coding", "sdtm-vs-epoch-from-subject-elements", "sdtm-lb-metadata")


def stage(destination: Path):
    destination.mkdir()
    shutil.copytree(
        ROOT / "rust/crates/yamaa-adapters/tests/fixtures/define-schemas",
        destination / "schemas",
    )
    for name in CASES:
        original = ROOT / "benchmarks" / name
        target = destination / name
        target.mkdir()
        shutil.copy2(original / "spec.yaml", target / "spec.yaml")
        if name == "sdtm-lb-metadata":
            shutil.copy2(original / "submission-spec.yaml", target / "submission-spec.yaml")
            shutil.copy2(original / "submission-environment.yaml", target / "environment.yaml")
            (target / "expected").mkdir()
            shutil.copy2(original / "expected/lb.csv", target / "expected/lb.csv")
        else:
            shutil.copy2(original / "environment.yaml", target / "environment.yaml")
        if name == "sdtm-dm-metadata":
            expected = target / "expected"
            expected.mkdir()
            for file in ("dm.csv", "dm.json", "define.xml"):
                shutil.copy2(original / "expected" / file, expected / file)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
