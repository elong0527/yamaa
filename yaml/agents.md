# YAML Configuration Layer

This directory is used to store YAML configuration files for the project.

**Agent Guidelines:**
- Ensure all YAML files are strictly valid and well-formatted.
- Keep configurations modular and clearly name files based on their purpose.


## schema.yaml

The `schema.yaml` file defines specifications for ODM to SDTM and SDTM to ADaM
derivations.

### Derivation model

Derivation occurs in two steps:

1. **Row construction** starts from the dataset declared by `base` and
   establishes the output rows. Each row is identified by the dataset `keys`.
   Row construction may change the number of rows.
2. **Column derivation** adds one value per key combination by left-joining on
   the dataset `keys`. Column derivation must not change the number of rows.

When multiple source records match one output key, the derivation must filter,
aggregate, or select one record before the join. Otherwise, it must fail.
