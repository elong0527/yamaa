"""Benchmarks the 1.0 conversion gate excepts until their decided change lands.

The conversion gate of #3030 Phase 1 (P1-9, #3259) converts every benchmark
to 1.0 and runs it: a positive must reproduce its expected bytes, and a
negative its condition, rule and context. Decided items change these cases,
so they are excepted until P2-4 and P1-11 land: D60 re-authors three
negatives' expected errors for their 1.0 fault, D61 retires two negatives
with their conditions, and under D32 `sdtm-lb-metadata` binds codelists
without a project. The selftest case list refuses them too
(`tools/validation/check_selftest_cases.py`), since their expected files are
about to change.
"""

EXCEPTED = {
    "negative-impute-missing-month": "D61: retires with month_required (P2-4)",
    "negative-impute-unreachable-month": "D61: retires with month_not_permitted (P2-4)",
    "negative-literal-structure": "D60: its expected error is re-authored (P2-4)",
    "negative-lookup-unknown-key": "D60: its expected error is re-authored (P2-4)",
    "negative-version-mismatch": "D60: its expected error is re-authored (P2-4)",
    "sdtm-lb-metadata": "D32: binds codelists without a project until P1-11",
}
