# Reject a Sponsor-Added Epoch That Claims a Published Code

**Goal:** build the trial arms domain, one record per planned element of
each arm, with its epoch (`EPOCH`) restricted to the study's epoch codelist.

**Input:** the protocol's planned arms and elements, and the study's
codelists in the project file: the published epoch list, which admits
sponsor additions, with the extension epoch added by the sponsor.

**Variables:**

- `DOMAIN` is `TA` on every record.
- `EPOCH` would be the epoch of the element, one of the codelist's values.

**Note:** the extension epoch is marked as a sponsor addition, yet it
carries a published code, the one the treatment epoch carries. A sponsor
addition has no published code, so the code claims a concept the published
list does not assign to it, and the run is rejected before any data is read;
no artifact is accepted.

## How to fix

Remove the code from the sponsor addition, and keep the mark that it was
added:

```yaml
- value: EXTENSION
  extended: true
```
