# Repository validation

The YAML Validation workflow checks authored specifications, schemas, rules,
fixtures, style and documentation. The validator uses the installed shared Rust
engine through public `yamaa.check` and bounded native project metadata services.
Native CSV/Parquet/ODM/regex and grammar fixtures replace the deleted
`_reference_checks.py` boundary. Repository schema/layout/editorial checks remain
in Python. See [development](../docs/development.md) for architecture and
[release](../docs/release.md) for final installed migration acceptance.

Automatic PR feedback uses the lightweight repository checks in
[`pr.yml`](workflows/pr.yml), plus selected Rust/Python smoke tests. Complete
semantic validation and deterministic dashboard generation run on main pushes
or manual `yaml-validation.yml` dispatches. The full scope below describes that
separate qualification workflow.

## Scope

| Check | Current responsibility |
| --- | --- |
| Sources and YAML | ASCII repository source, valid Unicode data, YAML 1.2 core resolution, duplicate-key/tag/anchor/alias refusal. `.github/` YAML is outside the repository YAML scan. |
| Schemas | Included files stay within `yaml/`, types/registries/constraints resolve and entry points validate. |
| Specifications | Native admission, inherited declarations, static bindings, original declaring-file paths, closed source metadata and producer contracts. Metadata capture does not grant project activation, derivation execution or Save authority. |
| Fixture diagnostics | `benchmarks/validation-manifest.yaml` declares negative cases, conditions, paths and blockers. Every implemented expected validation condition must appear at its declared paths; unrelated findings fail. |
| Benchmark inventory | Benchmark directories and the public-conformance configuration derive case membership, both host entries and explicit exclusions. `check_execution_manifest.py` checks that inventory, prior observations and the generated index; no execution manifest is maintained. |
| Sources and artifacts | Native source-profile checks, CSV quoting/missingness/header/field contracts, expected output names/types/representation and key order. Expected golden values are authored truth, not regenerated results. |
| Layout and prose | Required example files, negative remediation sections, variant layout, benchmark index, badges, line width, described columns and rule metadata. |
| Grammar and regex | Shared grammar/vocabulary vectors and pinned R022 regex conformance through native bindings. Missing bindings fail rather than falling back. |
| Environments and Define | `environment_class` and `define_class` schema validation for authored documents. Complete generation and saved-byte comparison belong to installed public suites. |
| Rules and references | Current normative contracts and schema navigation determine requirement ownership, citations and generated references. `test_rule_rewrite.py` checks the 363 independent owners in [schema-navigation.tsv](scripts/yaml-validation/fixtures/schema-navigation.tsv); historical mappings remain in Git. |

The historical `negative-path-absolute` and `negative-path-parent-escape` cases
remain explicit public-conformance exclusions. Four old resolved-spec expected
outputs were removed; no public resolved-spec API or required expected resolved
artifact remains. The validator does not execute clinical derivations or prove
golden output values. Full installed domain/check/Define, R grammar and
cross-platform parity evidence belongs to the native workflow and its independent
audit, not a repository-validation pass.

## Local commands

Run from the repository root. CI uses Python 3.14 and Rust 1.90.0:

```sh
uv sync --project python --extra test --locked
uv run --project python --no-sync python .github/scripts/yaml-validation/test_validate_repository.py
uv run --project python --no-sync python .github/scripts/yaml-validation/validate_repository.py --root .
uv run --project python --no-sync python .github/scripts/yaml-validation/check_documentation.py
uv run --project python --no-sync python .github/scripts/yaml-validation/test_rule_rewrite.py
uv run --project python --no-sync python .github/scripts/yaml-validation/check_rule_rewrite.py
uv run --project python --no-sync python .github/scripts/yaml-validation/generate_rule_reference.py --check
uv run --project python --no-sync python .github/scripts/yaml-validation/check_requirement_registry.py
```

The script infers the repository root from its location when `--root` is omitted.
It returns nonzero for errors; warnings print without failing unless
`--warnings-as-errors` is selected.

[Specification style](../rules/specification/style.md) is a separate authoring
check. It fixes field order, spacing, line width and accepted written spellings;
semantic authoring advice has no automatic fix. Layout fixes are kept only after
shared admission/equality accepts them and comments remain unchanged:

```sh
uv run --project python --no-sync python -m yamaa.style benchmarks/*/spec*.yaml
uv run --project python --no-sync python -m yamaa.style --fix benchmarks/<name>/spec.yaml
```

Shared grammar checks use the installed native package and pinned YAML reader.

The R grammar replay requires jsonlite 2.0.0 in its selected test library to read
the shared fixture truth.

```sh
uv run --project python --no-sync python rust/tools/check_shared_grammar.py
Rscript --vanilla R/yamaa/tests/public_grammar.R
```

CI also runs execution/vocabulary/metadata checks, generated-reference freshness,
requirement and prose tests, and authenticated validation-blocker issue reads.
The [workflow](workflows/yaml-validation.yml) is the complete command list;
[CI guidance](ci.md) describes canonical-page tests, scoped dashboard previews
and native qualification. No README copy-synchronization step exists.
