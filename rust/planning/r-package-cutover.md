# yamaa R source-package cutover

This is the concrete R removal record for
[#1993](https://github.com/elong0527/yamaa/issues/1993), alongside the public API
contract in [#1991](https://github.com/elong0527/yamaa/issues/1991). The removal
branch prepares removal until the [transition gates](transition.md) pass on the
combined main revision. Review readiness does not approve transition.

## Installation and source artifact

The approved first release channel is a GitHub source asset. The planned shared
release uses `yamaa_0.4.0.tar.gz`; the prepared development template is version 0.4.0.
Its source installation requires R 4.6.1, Rust 1.90.0/Cargo and platform compiler
and make tools. Windows x64 additionally requires Rtools45 and the GNU Rust
target. CRAN, r-universe and R binary distribution are later work.

`rust/tools/stage_r_package.py` stages the complete shared workspace, canonical
Cargo lock, original source fixtures, and public specification/project/Define/
grammar resources into a standalone archive tree. Staging never changes Cargo
resolution or overwrites an existing destination. The staging regression compares
every retained crate source/fixture byte, the lock and the authored grammar bytes.
The package builds with `cargo build --locked`; installed R needs no Python
runtime or repository checkout. Dead installed prototype resources are omitted.

The release must preserve the exact staged tarball as its release asset and link
its final revision, dependency/toolchain evidence and installed qualification.
The native workflow requires source build/install, all retained public suites and
strict `R CMD check` with `Status: OK` independently on Linux, macOS and Windows.
A passing Linux grammar workflow alone does not qualify a release.

## Legacy assertion disposition

| Legacy test file | Retained evidence or deliberate retirement |
| --- | --- |
| `test-grammar-conformance.R` | All 149 authored vectors in the four unchanged `yaml/grammar/*.yaml` documents retain exact decisions, shapes and identifier sets in `check_shared_grammar.py` through typed core APIs. Another 258 generated boundary checks cover declared function arity, numeric prohibited words, reducer argument forms, predicate reserved words and comparison operators. Two exhaustive native function/reducer inventories check closed vocabulary equality. No reference evaluator generates expected truth. |
| `test-delimited-empty-missing.R`, `test-sdtm-csv-ingestion.R` | Installed `R/yamaa/tests/public_grammar.R` preserves typeless leading-zero text and missing bare/quoted empty CSV fields through `yamaa_domain`, owned output and exact Save bytes. Original string-handler fixture output remains in public benchmark conformance; native CSV/typed-source tests retain byte and type boundaries. The old form/item-key configuration syntax retires. |
| `test-submission.R` | Native `submission_document`, `submission_values`, `submission_composition`, `specification_submission_document` and scalar/diagnostic submission suites retain current-schema metadata and codelist rules. Installed `public_define.R`, `public_environment.R` and public benchmark conformance retain authored metadata fixtures and exact rendered output. Old `load_study_document`/validation extension entry points and their separate R schema copies retire. |
| `test-common.R`, `test-dm_functions.R`, `test-vs_functions.R` | The clinical convenience functions retire under the approved API scope. Current-schema temporal and expression rules are covered by typed native fixtures and installed public benchmark truth; no replacement legacy helper API is promised. |
| `test-adam_engine.R`, `test-sdtm_engine.R`, `test-sql_derivation.R` | The legacy key/data-dependency/form mapping evaluator and its SQL/closest markers retire. Their private object/SQL shapes are outside the approved surface. Current-schema domain, joins, lookup, aggregates, ODM expressions and producer lifecycle remain required through public benchmark/native tests. |
| `test-odm_parser.R` | `parse_odm_to_long_df`, metadata extraction and the non-reader R ODM implementation retire as explicitly approved. The separately retained Python `read_odm`/`write_odm_parquet` helpers and native current-schema ODM source behavior require their own installed qualification; they are not aliases for the removed R parser. |

The installed R and Python grammar suites use the same 407 authored/expanded
texts in actual specification fields. Accepted grammar can still fail later name
binding or evaluation; exact parser structure is established by the typed core
replay. Empty expression fields fail schema admission under REQ-0287 before the
parser, while the typed replay preserves their grammar rejection. The suites
never save their grammar runs. There is no host grammar/probe API.

The replacement `r.yml` runs typed core grammar and an installed standalone
`R/yamaa` replay on Linux for PRs or explicit dispatch. Main pushes retain the
approved Rust/math policy. The complete installed matrix lives in `native.yml`.

## Retirement and rollback gates

The draft deletes the entire 66-file `R/cdiscbuilder` package, its 22-export API
inventory and evaluator CI consumer. Installation docs, both slide links, site
navigation and `.gitattributes` point to `R/yamaa`. `parse_odm_to_long_df` has a
removal disposition, with no compatibility translation or fallback.

The previous release source is pinned by tag `yamaa-py-0.3.0`, revision
`845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`. The measurement agent retained its exact
repository source archive before this removal. No previous R release was
published. Instead, the qualified installed pre-cutover R package at source
`a1a9c2d216075f331ac108aa4d3638573f103cea` is retained from audited run
38394553441. One exact archive passed fresh-library domain/check/Define and
post-input-removal Save rehearsal on Linux, macOS and Windows in run
38435902670 at preparation head
`d909928c94f308550c7f584c12b6b42506c4c84c`. Draft retention release 411979803,
tag `yamaa-r-reference-a1a9c2d2`, targets that exact source and remains
unpublished. Hosted run 38450032850 retained source asset 637944247 and receipt
bundle 637944307 after comparing the original audited artifact bytes, then
authenticated and compared the uploaded bytes. The complete
[asset receipt](../qualification/rollback-assets.json) records those identities.
A reviewed [retrievable study pin](rollback-pins.md) remains required. The old
`cdiscbuilder` package reports 0.1.0;
its source snapshot must not be described as an existing qualified `yamaa` 0.3.0
R artifact. Preserve the reference source and measured baseline until rollback,
performance budgets and transition approval are recorded.

Before merging this removal, require the combined final revision's complete
public conformance ratchet, retained helpers/public API suites, source-package
membership/locked-build evidence, strict three-platform R checks, representative
performance results, and concrete transition approval. Do not close #1993 from
branch-local parser/staging results or a passing sampled PR workflow.
