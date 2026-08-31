# Repository Validation

This document describes the validation scope and rules for the `yamaa`
specification repository. The repository provides a validation tool to ensure
examples, schemas, and directories are structurally consistent and well-formed.

## Scope
The validation ensures:
1. **YAML syntax**: YAML 1.2 core Boolean resolution, no duplicate mapping
   keys, anchors, aliases, merge keys, explicit tags, or syntax errors across
   any `.yaml` files.
2. **Schema integrity**: `schema.yaml` is required, and included files must
   stay inside `yaml/` and resolve without cycles. Custom types (`list[T]`,
   `dict[K,V]`, unions, classes, aliases, and registries) must resolve. A
   `portable_registry` reference must resolve to a regular file inside
   `yaml/`. The validator also enforces `values`, `pattern`, `min_length`, and
   `size` constraints.
3. **Example specs**: Every example `spec.yaml` validates against the schemas,
   checking required fields, unknown fields, and registry payload shapes.
   Negative examples (folders prefixed with `negative-`) are structurally
   validated; structural errors are only suppressed if their named path matches
   a `spec_path` declared in `expected/error.yaml` with `phase: validation`.
4. **Layout**: All examples have `README.md`, `spec.yaml`, `input/`, and
   `expected/`. Negative examples must provide `expected/error.yaml` and a `##
   How to fix` section. Positive examples must not provide
   `expected/error.yaml`.
5. **CSV consistency**: Expected output CSV files must match exactly the
   `output.columns` sequence declared by the specification.
6. **Example Index**: `yaml/examples/README.md` must accurately list all
   example directories in alphabetical order without stale entries. The
   descriptions must match the contract defined by the first line of the
   example's `README.md`.

## Explicit Non-Goals
The validator only ensures structural correctness. At this time, it **does
not**:
- Execute clinical derivations.
- Materialize shorthand canonical forms (no transformed document is returned).
- Reproduce golden output values in the `.csv` files.

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
Warnings are printed to standard output but do not fail validation. One
existing DS example warns until its two column verifications are represented as
a list of one-entry mappings, as R006 requires. This narrowly scoped migration
warning does not permit the same structural error anywhere else. Column-label
and dependency-order policies remain owned by the existing Ruby checks under
`.github/workflows/`.

To treat warnings as errors, run with the `--warnings-as-errors` flag:

```bash
python3 .github/workflows/validate_repository.py --warnings-as-errors
```
