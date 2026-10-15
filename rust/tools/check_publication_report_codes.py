"""Keep pre-owned report codes exhaustive over the actual publication authorities."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
AUTHORITIES = (
    "rust/crates/yamaa-engine/src/producer_publication.rs",
    "rust/crates/yamaa-adapters/src/producer_publication.rs",
    "rust/crates/yamaa-adapters/src/producer_log_publication.rs",
    "rust/crates/yamaa-adapters/src/producer_result.rs",
)
REPORT = "rust/crates/yamaa-adapters/src/report_publication.rs"
CORE = "ConditionCode::OutputPublicationFailed"


def missing_codes(authorities, report):
    actual = set()
    for source in authorities:
        actual.update(re.findall(
            r'(?:Failure::Limit|AdmissionFailure::Limit)\(\s*"([a-z_]+)"', source))
        actual.update(re.findall(r'\(\s*false\s*,\s*"([a-z_]+)"', source))
        actual.update(re.findall(
            r'(?:ok_or|Err)\(\s*"(producer_publication_[a-z_]+|publication_report_limit)"',
            source))
    block = re.search(r'const CODES: \[&str; \d+\] = \[(.*?)\];', report, re.S)
    owned = set(re.findall(r'"([a-z_]+)"', block.group(1))) if block else set()
    missing = actual - owned
    if any(CORE in source for source in authorities) and CORE not in report:
        missing.add(CORE)
    return missing


def check(repository=ROOT):
    return missing_codes([(repository / path).read_text() for path in AUTHORITIES],
                         (repository / REPORT).read_text())


if __name__ == "__main__":
    missing = check()
    if missing:
        raise SystemExit("publication report codes missing before effects: " +
                         ", ".join(sorted(missing)))
    print("PASS: pre-owned report codes cover actual publication failure authorities.")
