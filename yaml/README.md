# YAML derivation specification

This folder defines a compact, language-agnostic specification for ODM-to-SDTM
and SDTM-to-ADaM derivations. The design is under active development.

## Contents

- `schema.yaml` is the schema-bundle entry point and defines shared structure.
- `schema_derivation.yaml`, `schema_expression_*.yaml`, and
  `schema_verification.yaml` register closed derivation and verification types.
- `rules/` contains the execution semantics, with one rule per file.
- `examples/` contains source data, derivation specifications, and exact
  expected outputs.
- `agents.md` tells AI coding agents how to discover and maintain the design.

The rule files are the authoritative source for behavior. The schema defines
shape, while examples demonstrate rules without redefining them.

## Review workflow

1. Review the root field in `schema.yaml` and its included schema module.
2. Review every applicable rule listed in `rules/README.md`.
3. Review at least one positive fixture and its expected output.
4. Add a negative fixture when the rule defines an error condition.
5. Require R and Python implementations to produce equivalent outputs and
   errors from the same fixtures.

Behavior not defined by a normative rule must not be inferred by an
implementation. It should be proposed as a new rule or marked as an unresolved
design question.
