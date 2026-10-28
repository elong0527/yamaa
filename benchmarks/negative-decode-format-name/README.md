# Reject a Parameter Decoded Through a Format Name

**Goal:** derive the parameter name (`PARAM`) of each laboratory result
from its parameter code (`PARAMCD`), as the study's parameter terminology
decodes the code.

**Input:** collected laboratory results with the study and subject
identifiers and the test code, which is the parameter code.

**Variables:**

- `PARAM` would be the name the study's parameter terminology gives the
  code, such as `Alanine Aminotransferase (U/L)` for `ALT`.

**Note:** the study declares its parameter terminology once, under a name
a specification refers to it by. `$PARAMCD` is the SAS format a reader of
the submission applies to the codes, not that name, and no terminology
name begins with a dollar sign, so the request is rejected before any
record is read and no artifact is accepted.

## How to fix

Refer to the terminology by the `id` the study's project file declares it
under, `PARAMCD`, rather than by its `format_name`, and build the dataset
from that project file, which declares the codes and their decodes once:

```yaml
yamaa: "1.0"
type: project
codelists:
  - codelists:
      - id: PARAMCD
        name: Parameter Code
        format_name: $PARAMCD
        items:
          - {value: ALT, decode: Alanine Aminotransferase (U/L)}
          - {value: AST, decode: Aspartate Aminotransferase (U/L)}
adam:
  specs: [spec.yaml]
```

```yaml
derivation:
  decode:
    source: PARAMCD
    codelist: PARAMCD
```

Each code then takes the decode its item declares, and a code the
terminology does not list stops the run unless an `unmapped` answer is
declared.
