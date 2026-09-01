# Repository Validation

This document describes the validation scope and rules for the `yamaa`
specification repository. The repository provides a validation tool to ensure
examples, schemas, and directories are structurally consistent and well-formed.

## Scope
The validation ensures:
1. **YAML syntax**: YAML 1.2 core scalar resolution, no duplicate mapping keys,
   anchors, aliases, merge keys, explicit tags, or syntax errors across any
   repository `.yaml` files outside `.github/`.
2. **Schema integrity**: `schema.yaml` is required, and included files must
   stay inside `yaml/` and resolve without cycles. Custom types (`list[T]`,
   `dict[K,V]`, unions, classes, aliases, and registries) must resolve. The
   validator also enforces `values`, `pattern`, `min_length`, and `size`
   constraints.
3. **Example specs**: Every `spec.yaml` or `spec_<variant>.yaml` validates
   against the schemas, checking required fields, unknown fields, and registry
   payload shapes. Cross-field validation rejects duplicate or unresolved
   dataset, lookup, column, key, output-column, and row names, including
   namespace conflicts. A grouped row must declare a non-empty, duplicate-free
   `group_by` whose variables are qualified to that row's driver. Static
   cross-field checks enforce base selection, complete and exclusive
   derivation coverage, record-lookup field pairing, verification bounds and
   IDs, source-file existence, and declared CSV fields. Referenced source
   producing specifications referenced through `schema` validate recursively
   against `root_class`; producer paths and derivations must resolve, workflow
   dependencies must be acyclic, every stored producer column must have a
   non-empty label, and the stored CSV header must match the producer's ordered
   `output.columns` exactly. A producing specification cannot be combined with
   inline `types`.
   Negative examples (folders prefixed with `negative-`) are structurally
   validated; structural errors are only suppressed if their named path
   matches a `spec_path` declared in `expected/error.yaml` with
   `phase: validation`.
4. **Layout**: All examples have `README.md`, one `spec.yaml` or one or more
   `spec_<variant>.yaml` files, `input/`, and `expected/`. A base spec cannot
   be mixed with variants. Negative examples must provide `expected/error.yaml`
   and exactly one `## How to fix` section. Positive examples must not provide
   `expected/error.yaml`. Error contracts use the closed phase vocabulary,
   snake-case conditions, existing specification paths, and an optional
   mapping context.
5. **CSV consistency**: Input and expected CSV files must have unique,
   non-empty headers and a consistent field count. Expected output headers
   must match exactly the `output.columns` sequence declared by the
   specification.
6. **Example Index**: `yaml/examples/README.md` must accurately list all
   example directories in alphabetical order without stale entries. The
   descriptions must match the contract defined by the first line of the
   example's `README.md`.
7. **Example documentation**: Data contracts must stay within 79 columns,
   avoid schema vocabulary, describe each non-key expected column, and use
   only the remediation or specification-variant sections allowed by
   `yaml/examples/agents.md`.

## Explicit Non-Goals
The validator ensures structural correctness and the static cross-field checks
listed above. At this time, it **does not**:
- Execute clinical derivations.
- Materialize shorthand canonical forms (no transformed document is returned).
- Reproduce golden output values in the `.csv` files.
- Parse or type-check the numeric, aggregate, template, or predicate leaf
  languages; this is tracked in issues #103 and #98.
- Prove that a regular expression behaves identically in R and Python; the
  ECMAScript portability contract is tracked in issue #106.

## Local Commands
To run the validator locally:

```bash
python3 .github/workflows/validate_repository.py --root .
```

By default, the script infers the repository root relative to its own path.

## Exit Behavior
- Returns `0` if the repository structure is completely valid (no errors).
- Returns non-zero (e.g. `1`) if any errors are encountered.

## Warning Policy
Warnings are printed to standard output but do not fail validation.
Column-label and dependency-order policies remain owned by the existing Ruby
checks under `.github/workflows/`.

To treat warnings as errors, run with the `--warnings-as-errors` flag:

```bash
python3 .github/workflows/validate_repository.py --warnings-as-errors
```
