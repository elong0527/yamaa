"""Copy original shared Define metadata and independent fixed-byte witnesses."""
import argparse
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CASES = ("sdtm-dm-metadata", "sdtm-ae-coding", "sdtm-vs-epoch-from-subject-elements")


def stage(destination: Path):
    destination.mkdir()
    for name in CASES:
        original = ROOT / "benchmarks" / name
        target = destination / name
        target.mkdir()
        for file in ("environment.yaml", "spec.yaml"):
            shutil.copy2(original / file, target / file)
        if name == "sdtm-dm-metadata":
            expected = target / "expected"
            expected.mkdir()
            for file in ("dm.csv", "dm.json", "define.xml"):
                shutil.copy2(original / "expected" / file, expected / file)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
