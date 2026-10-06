# Supplemental contract evidence

The benchmark inventory answers which original specifications ran completely.
The supplemental catalog answers which service contracts and installed probes
exist and which of those probe suites actually ran. These are different claims.

`qualification/supplemental.json` explicitly lists shared fixture contracts,
supporting inputs/documentation, known consumer source files and installed Python/R
suite identities. The inventory reconciles every file under each Rust crate's
`tests/fixtures/` tree and every `rust/tests/installed_*.py` and
`R/yamaanative/tests/*.R` script. Added, removed or duplicate entries require an
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
