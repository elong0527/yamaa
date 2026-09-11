- Normative changes are governed: a rule ID is reserved in the
  `Reserved rule IDs` table of `yaml/rules/README.md` on `main` before the
  branch that writes its file, `yaml/rules/` and `yaml/schema*.yaml` carry
  code owners, and every normative change adds a `changelog.d/` entry.
  Repository validation rejects a tree where two files claim one rule ID,
  where an indexed ID has no file, or where an ID is both reserved and
  defined; a pull request check rejects a rule ID that was not reserved on
  the base branch and a normative change with no entry. (#174)
