# yamaa <img src="docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

yamaa is a domain-specific language (DSL) for clinical trial data standardization.
The goal is to transform ODM XML data, extracted from an EDC system,
into SDTM and ADaM datasets following CDISC standards. Rules fix what
every item means, so the same specification with the same inputs always
produces the same dataset.

The language is written to be read and revised by people and AI agents
together, keeping derivations reviewable, version-controlled, and consistent
across implementations. The yamaa project has four components: schema, rules, engine, and
benchmark.

| Component | Purpose | Repository |
|---|---|---|
| Schema | Declares the vocabulary. | [`yaml/`](yaml/) |
| Rules | Fix the meaning of the schema. | [`rules/`](rules/) |
| Engine | Runs specifications in Python and R | [`python/`](python/), [`R/`](R/) |
| Benchmark | Runnable examples. | [`benchmarks/`](benchmarks/) |

## From ODM XML to SDTM and ADaM

The engine reads the ODM XML extracted from the EDC system 
and projects it into one long-form table with fixed schema ([example](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-basic.html)).  
A yamma specification reads that projection and derives SDTM then ADaM
columns onto the rows it constructs. 

## Inheritance

![yamaa design: inherited templates become study specifications that drive validated SDTM, ADaM](docs/diagrams/design.svg)

Reusable yamaa specification templates flow from the organization level through the compound and
study levels. Approved study specifications then drive deterministic, validated
builds while preserving metadata lineage.

## Example

The specification is deterministic by design and supports SQL expressions. More realistic examples are available in the [`benchmarks/`](benchmarks/) directory.

```yaml
- name: BMI
  type: float
  label: Body Mass Index (kg/m2)
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```
