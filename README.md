# yamaa <img src="docs/assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

YAMAA is a domain-specific language (DSL) for clinical trial data standardization.
A YAMAA specification transforms ODM XML data, extracted from an EDC system,
into SDTM and ADaM datasets following CDISC standards. YAMAA's rules fix what
every item means, so the same specification with the same inputs always
produces the same dataset.

The language is written to be read and revised by people and AI agents
together, keeping derivations reviewable, version-controlled, and consistent
across implementations. It has four components: schema, rules, engine, and
benchmark.

| Component | Purpose | Repository |
|---|---|---|
| Schema | Declares the vocabulary of the language: what a specification may contain. Anything the schema does not declare is rejected before execution. | [`yaml/`](yaml/) |
| Rules | Fix the meaning of every written item, so the R and Python engines execute the same specification in exactly one way. | [`yaml/rules/`](yaml/rules/) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [`python/`](python/), [`R/`](R/) |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs. | [`benchmark/`](benchmark/) |

## From ODM XML to SDTM and ADaM

The engine reads the ODM XML extracted from the EDC system -- a plain file or a
TAR archive -- and projects it into one long-form clinical-item table: one row
per recorded item, keeping that item's study, event, form and item-group
context. A specification reads that projection and derives SDTM and ADaM
columns onto the rows it constructs. Benchmarks ship the projection directly as
a small `odm.csv`, so each example stays reviewable by eye.

## Design

![YAMAA design: inherited templates become study specifications that drive validated SDTM, ADaM](docs/diagrams/design.svg)

Reusable templates flow from the organization level through the compound and
study levels. Approved study specifications then drive deterministic, validated
builds while preserving metadata lineage. Their ordered, shallow composition
and minimal resolved form are defined by
[specification composition](yaml/rules/specification/composition.md).

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
