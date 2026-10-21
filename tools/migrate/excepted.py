"""Benchmarks the 1.0 conversion gate excepts until their decided change lands.

The conversion gate of #3030 Phase 1 (P1-9, #3259, `make check-conversion`)
converts every benchmark to 1.0 and runs it: a positive must reproduce its
expected bytes, and a negative its condition, rule and context, with spec
paths changed only by the rename table's `paths` block. A decided item that
changes a case excepts it until its pull request lands; P2-4 landed D60's
three re-authored negatives and retired the D61 and D137 negatives, so what
remains waits for P2-7. The selftest case list refuses every excepted
benchmark too (`tools/validation/check_selftest_cases.py`), since their
expected files are about to change.
"""

EXCEPTED = {
    "negative-date-diff-units": (
        "D58 (P-ORD-2): its context names diff.bounds values, which 1.0 renames and "
        "TIME-5 names as closed values (D71); P2-7 re-authors it"
    ),
}
