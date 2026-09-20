# yamaa <img src="docs/assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

YAMAA is a language-neutral YAML specification for reproducible clinical trial data pipelines that transform ODM data into SDTM and ADaM datasets following CDISC standards.

YAMAA is designed for AI-agent and human collaboration on clinical data standardization, keeping derivations reviewable, version-controlled, and consistent across implementations. It has four components: schema, rules, engine, and benchmark.

| Component | Purpose | Repository |
|---|---|---|
| Schema | Declares what a specification may contain; anything the schema does not declare is rejected before execution. | [`yaml/`](yaml/) |
| Rules | Fix the meaning of every written item, so the R and Python engines execute the same specification in exactly one way. | [`yaml/rules/`](yaml/rules/) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [`python/`](python/), [`R/`](R/) |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs. | [`benchmark/`](benchmark/) |

## Design

![YAMAA design: inherited templates become study specifications that drive validated SDTM, ADaM](docs/diagrams/design.svg)

Reusable templates flow from the organization level through the compound and
study levels. Approved study specifications then drive deterministic, validated
builds while preserving metadata lineage. Their ordered, shallow composition
and minimal resolved form are defined by
[R017 specification inheritance](yaml/rules/R017-specification-inheritance.md).

## Repository

- [`yaml/`](yaml/) - schemas and execution rules
- [`benchmark/`](benchmark/) - runnable examples with exact expected output
- [`R/`](R/) - R implementation and workflows
- [`python/`](python/) - Python implementation
- [`docs/`](docs/) - diagrams and assets

## Example

The specification is deterministic by design and supports SQL expressions. More realistic examples are available in the [`benchmark/`](benchmark/) directory.

```yaml
- name: BMI
  type: float
  label: Body Mass Index (kg/m2)
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```
