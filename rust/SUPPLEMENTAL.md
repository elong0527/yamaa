# Supplemental contract evidence

The benchmark inventory answers which original specifications ran completely.
The supplemental catalog answers which typed service contracts and installed public suites
exist and which of those suites actually ran. These are different claims.

`qualification/supplemental.json` explicitly lists shared fixture contracts,
supporting inputs/documentation, known consumer source files and installed Python/R
suite identities. The inventory reconciles every file under each Rust crate's
`tests/fixtures/` tree and every `rust/tests/installed_*.py` and
`R/yamaa/tests/*.R` script. Added, removed or duplicate entries require an
explicit catalog update. Suite IDs use `host/simple-name` so distinct suites
cannot overwrite each other's log filenames. JSON and TSV forms of independent expectations remain
separate file identities; neither is inferred from the candidate engine.

`tools/supplemental_inventory.py` validates this catalog and runs the existing
installed scripts in fresh processes outside the checkout. It checks staged
script bytes directly against the named checkout before running any suite.
Installed package metadata supplies actual runtime/package/core versions, and
CI supplies the tested checkout, package-form/artifact identity and run link.
No content digests are introduced.

A report retains each suite's exit code and log. Failure stops subsequent suites;
those entries remain `not_exercised`. A successful catalog reconciliation itself
contains no pass claims. A successful suite is evidence only for the assertions
that suite executes; a cataloged consumer path is not proof that every contract
ran. Rust-only and reference-only consumers remain linked source evidence rather
than being relabeled as installed-host passes. Existing Cargo debug/release,
reference Python, grammar and numerical assessment jobs remain necessary gates.

`component` includes typed execution, ownership and host bridge checks.
`compile` includes typed syntax/schema/inheritance/binding services. Neither
level substitutes for public `shared_run` qualification. Inline assertions are represented by their
suite identity. The separate complete-run inventory supplies the actual benchmark
claim and preserves its independent artifact/observation comparisons.

The native CI matrix retains supplemental reports and logs after both Python
wheel installation and source rebuild and after R source installation. These
commands replace the handwritten lists of the same scripts; they do not remove
assertions, regenerate fixtures or relax the whole-run regression gates.

The `python/public-contract` and `r/public-contract` suites exercise installed
domain/check/define entry points with independently authored inputs and truth.
They pin signatures, argument errors, JSON-text issue tables, immutable result
attributes, repeat builds and saves, owned views, integer/text representations,
activation interrupts and callback reentrancy. A failed verification preserves
its evaluated diagnostic log and permits its explicit diagnostic Save under
[REQ-1177](../rules/execution/verification.md#req-1177); a warning artifact is
built only for successful execution under
[REQ-0396](../rules/execution/verification.md#req-0396). Existing output and
warning files survive failed diagnostic saves. No private result probe is used
by these suites. Callback capabilities are replaced only to inject an interrupt
or reentrant invocation through the public facade. The existing environment,
producer and log suites retain their broader failure and publication coverage.

R argument shape and byte-encoded path failures raise `yamaa_argument_error`.
Missing files remain diagnostic rows. Full-range `yamaa_int64_vector` and
NUL-containing `yamaa_utf8_vector` preserve values and distinct missingness;
integer conversions are exact or fail. These approved contracts follow
[#1973](https://github.com/elong0527/yamaa/issues/1973) items 2-4. The suite
catalog and successful local execution do not replace exact-commit qualification
on all required Python wheel/source and R source platforms, including Windows R.

## Component endpoint retirement

#1990 removes every installed prototype endpoint and request protocol. The 26
remaining installed suites qualify public domain/check/define and retained typed
lifecycle ports. Python `_CORE_VERSION` and R's private `.core_version()` expose
compiled package metadata without the former capabilities/installation probe.
The frozen assisted inventory is historical evidence; native CI and its audit
use the public benchmark ratchet and installed supplemental evidence.

| Retired family | Assertion disposition |
| --- | --- |
| Installation/capabilities and scalar/1 | Remove endpoint/envelope assertions. Typed `scalar_bytes`, `public_table` and installed public-contract suites preserve full i64, nonfinite normalization, Unicode/NUL and separate missingness. |
| numeric/1 | Remove request limits/coercion assertions. Core numeric compiler, engine numeric lifecycle and shared arithmetic/literal fixtures preserve semantics and failure ordering. Installed math decision replay now uses public domain. |
| numeric/predicate/aggregate syntax | Private typed parser examples preserve complete AST, source position and differential comparisons; 409 shared grammar checks and 407 installed public checks preserve independent YAML grammar truth. |
| regex/1 | Remove installed protocol assertions. Typed core regex suite and private development-only regex observations retain independent R022 and differential assessment. |
| dependency/column dependency | Remove probe envelope assertions. Typed core dependency suites continue to consume the same independent TSV truth. |
| reference/1 | Remove installed metadata-query interfaces. Typed core reference binding/scope, intermediate, key, match-value and relation suites plus original public specifications preserve semantic assertions. |
| schema/1 and yaml/1 | Remove installed tree/query interfaces. Typed schema capture, preparation, diagnostic, bundle, normalization, layer, window and inheritance tests preserve sources/defaults/ordering. Historical schema citation fixtures remain test-only evidence. |
| inheritance/1 | #1988 replaces the callback with typed inheritance outcomes. Installed private inherited-document ports retain original errors/effects; public specifications qualify the production path. |
| table inspection/round trip | Remove installed endpoints. Seven adversarial IPC, masked payload, ownership, schema/shape, UTF-8 and expansion checks use typed IPC decoding and a private fixture serializer. Public outputs qualify installed ownership. |
| dataset/1 and function/1 | Remove JSON plans and explicit callable endpoints. Engine dataset/compiler/function-port suites and installed public environment/producer/lifecycle suites retain semantic, effect, interruption, recovery and result assertions. Typed successors explicitly preserve reducer source provenance, missing-only reductions and wide-count/projection refusals. |
| dataset profile/1 | Remove prototype measurement endpoint/harness. Public lifecycle measurement retains actual public execution, raw observations and process-failure evidence. |

No JSON request or reply crosses a host callback boundary. Production execution
and authored-source findings use the five fields phase, condition, requirement,
spec_paths and context. Installed issue context remains JSON text; retained
report observations use the same fields with their admitted context object.
Retired malformed-envelope tests have no surviving API to qualify. No benchmark
expected bytes or the #1974 benchmark inventory are changed by retirement.

The filesystem suites use public domain/check and actual files in both installed
hosts, with Python reference imports forbidden and Python absent from R's PATH.
Eight original physical capture configurations now exercise public inspection
before study reads. The seven unchanged inspection cases run twice, including
malformed surviving files which must remain unread. Fresh builds reinspect
changed or absent files, while earlier results retain their complete reports,
owned output and two exact Saves after all original files disappear. Public
parent-relative and absolute paths follow #1905; physical symlinks retain their
native refusal, with an explicit unavailable-control record when the host cannot
create one.

Private capture-stage partial reports and cached-source accounting still consume
all eight `source-capture.tsv` rows in typed `specification_run` tests and the
installed original-specification suite. The native engine's reused-build tests
pin inspection/capture ordering. Typed `file_resources` tests pin exact shared
snapshot ownership, byte ceilings, cached-content changes and unchanged capture
counts. Those private facts require no installed component endpoint or Python IO
implementation. The new public suites do not relabel their fresh builds as cached
captures or retain historical project containment as public behavior.
