# Benchmark examples

Each benchmark pairs a yamaa specification with small inputs and an exact
expected output or error. The fixtures support human review, automated tests,
and AI-assisted implementation.

Fixtures use CSV to stay easy to review and are not duplicated in Parquet.
Production artifacts should use `parquet` unless they must be reviewable as CSV.

`odm.csv` is a tabular projection of ODM clinical data, not an ODM exchange
document itself. Its fields map to the official
[CDISC ODM 2.0 clinical-data schema](https://github.com/cdisc-org/DataExchange-ODM/tree/main).

Start with [`sdtm-dm-basic`](sdtm-dm-basic/) for direct mapping,
[`sdtm-lb-findings`](sdtm-lb-findings/) for row construction, and
[`adam-adlb-bds`](adam-adlb-bds/) for a BDS derivation.

## Lifecycle

Each example carries a lifecycle badge showing its review state:

- `draft`: first commit.
- `reviewed`: has discussion comments or GitHub issues.
- `finalized`: human decision to finalize.

## Benchmarks

The index table for the authored cases is generated at docs build time from
the case directories themselves (`.github/scripts/benchmark-docs/build_index.py`);
it is not committed. The generated gallery on the project site lists every
case with its derivation, lifecycle and public host entries, and
`build_index.py --output <path>` renders the full Markdown index locally.

Each active case uses the same public Python and R plan. Actual passing
coverage is gated by the native run: every active benchmark must pass on
every lane.
