# yamaa <img src="docs/assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

YAMAA is named for *yama*, the Japanese word for mountain: collected data
narrows as it rises from collection through tabulation to analysis, the
same funnel from ODM through SDTM to ADaM.

YAMAA is a language-neutral YAML specification for reproducible clinical trial data pipelines that transform collected clinical data into SDTM and ADaM datasets following CDISC standards. ODM XML is a planned input: today the language reads a CSV tabular projection, and the rule that produces that projection from ODM XML is not yet specified.

YAMAA is designed for AI-assisted authoring while keeping derivations reviewable, version-controlled, and consistent across implementations: an agent can draft a specification from a study design, and a reviewer checks the same YAML the implementations execute.

## Design

![YAMAA design: inherited templates become study specifications that drive validated SDTM, ADaM](docs/diagrams/design.svg)

Reusable templates flow from the organization level through the compound and
study levels. Approved study specifications then drive deterministic, validated
builds while preserving metadata lineage. Their ordered, shallow composition
and minimal resolved form are defined by
[R017 specification inheritance](yaml/rules/R017-specification-inheritance.md).

## Repository

- [`yaml/`](yaml/) - schemas, execution rules, and examples
- [`cdiscbuildeR/`](cdiscbuildeR/) and [`R/`](R/) - R implementation and workflows
- [`python/`](python/) - Python implementation
- [`docs/`](docs/) - published site, diagrams, and authoring guides

## Example

The specification is deterministic by design and supports SQL expressions. More realistic examples are available in the [`yaml/examples/`](yaml/examples/) directory.

```yaml
- name: BMI
  type: float
  label: Body Mass Index (kg/m2)
  derivation:
    compute:
      expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```
