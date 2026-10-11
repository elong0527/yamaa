# Supplemental contract evidence

The benchmark inventory answers which original specifications ran completely.
The supplemental catalog answers which service contracts and installed probes
exist and which of those probe suites actually ran. These are different claims.

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

`component` includes typed-plan execution, transport, ownership and host bridge
checks. `compile` includes syntax/schema/inheritance/binding services and may
exercise temporary host orchestration. Neither level asserts a complete shared
compiler or satisfies `shared_run`. Inline assertions are represented by their
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
