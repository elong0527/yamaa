# Changelog entries

One file per normative change. A release assembles these files into
[`../CHANGELOG.md`](../CHANGELOG.md) and empties this directory.

Name a file `<issue>-<slug>.md`: the number of the issue the change closes,
then lowercase words joined by hyphens, for example
`174-normative-change-governance.md`. Write one or more markdown list items
stating what changed in the contract and what an implementation must now do
differently. Name the rules and schema modules the change touches.

`.github/scripts/yaml-validation/check_normative_change.py` requires a file
here whenever a pull request changes a rule file or a schema module.
