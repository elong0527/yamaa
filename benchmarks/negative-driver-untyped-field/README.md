# Reject a Dose Group Written as a Number and as Text

**Goal:** keep one record per study drug administration with a dose, and
give each a dose group (`DOSEGRP`).

**Input:** collected exposure records, one per administration, with the
dose given (`EXDOSE`).

**Variables:**

- `EXSEQ` would be the collected sequence number of the administration.
- `DOSEGRP` would be the dose group: high for a dose of 100 or more, low
  otherwise.

**Note:** the selected administrations build the records, and each carries
its dose group. The high group is written as the number 1 and the low group
as the text `low`, so the group has no single kind of value that the records
could be built with. The run is rejected before any data is read, and no
artifact is accepted.

## How to fix

Decide how the dose group is written, and write every group the same way.
As text:

```yaml
derivations:
  DOSEGRP:
    case:
      - when: "EXDOSE >= 100"
        then: "'high'"
      - otherwise: "'low'"
```

To keep a number, give the derived field a declared type of its own with
`{type: int, derivation: ...}`, and write the low group as a number too.
