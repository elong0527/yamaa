# Contributing

YAMAA is a specification. Its rules and schema bundle are the contract two
independent implementations are held to, so a change to them is governed
more tightly than a change to a script, a document, or an example.

Read [`yaml/agents.md`](yaml/agents.md) before changing anything under
`yaml/`: it states the maintenance rules, the ASCII source requirement, and
the reading order for the schema and the rule set.

## What counts as a normative change

A normative change is an edit to either:

- a rule file `yaml/rules/R<nnn>-*.md`, or
- a schema module `yaml/schema*.yaml`.

These are the paths whose meaning an implementation must follow. Changing
them means changing what a conforming implementation must do, whether the
edit adds a rule, weakens one, or corrects a sentence that two runtimes
could read differently.

The rule index `yaml/rules/README.md` is bookkeeping about the rule set
rather than a rule, so editing it alone (to reserve an ID, or to correct a
dependency column) is not a normative change. Examples under
`yaml/examples/` are governed by
[`yaml/examples/agents.md`](yaml/examples/agents.md); the golden artifacts
there pin behavior, but the conformance contract that makes them executable
is still open in issue #101.

## Reserving a rule ID

Rule IDs are stable and are allocated on `main`, never on a branch. Two
branches once allocated `R020` at the same time, and the second one had to
be renumbered by hand after the merge conflicted.

1. Open a pull request against `main` that adds one row to the
   `Reserved rule IDs` table in [`yaml/rules/README.md`](yaml/rules/README.md):
   the next free ID, the rule you intend to write, and the issue that asks
   for it. Change nothing else.
2. Merge it before, or alongside, opening the branch that writes the rule.
3. On that branch, delete the reservation row and add the rule's row to the
   index table above it, with `status: normative`.

Repository validation rejects a tree where two files claim one rule ID,
where an indexed ID has no file, or where an ID is both reserved and
defined. The pull request check additionally rejects a rule file whose ID
was neither indexed nor reserved on the base branch, which is what makes
step 1 unavoidable.

A rule ID never changes when its file is renamed, so a retitled rule keeps
its ID and needs no reservation.

## Recording a normative change

Every normative change adds one file to [`changelog.d/`](changelog.d/),
named `<issue>-<slug>.md`. The directory holds one file per change so that
branches landing in parallel never edit the same lines; a release assembles
them into [`CHANGELOG.md`](CHANGELOG.md).

Write what changed in the contract and what an implementation must now do
differently, and name the rules and schema modules involved.
[`changelog.d/README.md`](changelog.d/README.md) states the format.

What a release is, and which changes force a schema bundle version action,
is not settled: issue #104 closed without merging its release and
compatibility policy, the repository carries no tag, and bundle `1.0` is
neither declared released nor declared prerelease. Until that policy lands,
entries accumulate under `changelog.d/` and nothing is assembled.

## Review

[`.github/CODEOWNERS`](.github/CODEOWNERS) records who owns the rule set and
the schema bundle. GitHub requests a review from the owner on every pull
request touching those paths.

Ownership is recorded but not enforced: requiring a code-owner approval
needs a branch protection rule on `main`, and GitHub does not let anyone
approve their own pull request. The repository has one maintainer, who also
opens the automation-authored pull requests, so such a rule would block
every normative change rather than review it.

Once a second person can approve, enable it under
**Settings > Branches > Branch protection rules** for `main`:

- Require a pull request before merging
- Require approvals: 1
- Require review from Code Owners
- Require status checks to pass: `repository-validation (ubuntu-latest)`,
  `repository-validation (macos-latest)`, `normative-change`

Until then, the mechanical gates below are what CI enforces, and they are
all satisfiable by one person.

## Before opening a pull request

Run the checks CI runs, from the repository root, on Python 3.14:

```bash
python3 -m pip install -r .github/scripts/yaml-validation/requirements.txt
python3 .github/scripts/yaml-validation/test_validate_repository.py
python3 .github/scripts/yaml-validation/validate_repository.py --root .
python3 .github/scripts/yaml-validation/check_execution_manifest.py
ruby .github/scripts/examples/test_check_example_dependencies.rb
ruby .github/scripts/examples/check_example_dependencies.rb
ruby .github/scripts/examples/check_labels.rb
asciilint .
git diff --check
```

A normative change also clears the governance gates. They compare against a
base branch, so pass the branch the pull request targets:

```bash
git fetch origin main
python3 .github/scripts/yaml-validation/check_normative_change.py \
    --base origin/main
```

[`.github/yaml-validation.md`](.github/yaml-validation.md) documents what
the repository validation covers and what it deliberately does not.
