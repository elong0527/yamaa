# Reviewer feedback recipe

Canonical recipe for turning a reviewer's finding into a public yamaa GitHub
issue without exposing confidential information. An AI coding agent runs it
for the reviewer, inside the reviewer's own environment, and the reviewer
approves everything that leaves it.

- everything in `elong0527/yamaa` is public: issues, comments, attachments,
  pull requests, discussions, and their edit history
- what reaches GitHub is written from public sources and invented data,
  never edited down from confidential material
- the reviewer, not the agent, decides that a draft may be published
- yamaa has no private intake channel; evidence that cannot be made public
  stays inside the reviewer's organization

This recipe controls what reaches GitHub, not what reaches the agent. Run it
only with an agent your organization approves for the material it reads.

## 1. Scope

- **Who.** A reviewer is anyone checking yamaa against their own studies,
  specifications, or data: a statistician, a programmer, a QA reviewer, or a
  pilot team following [`sdtm_to_adam.md`](sdtm_to_adam.md).
- **What.** A new issue, and every later comment, reply, attachment, or pull
  request that follows it.
- **Which template.** Write the draft in the shape of the matching form in
  `.github/ISSUE_TEMPLATE/`:

| Finding | Template | Label |
|---|---|---|
| the engine crashes, or a rule and a file disagree | `bug-report.yml` | `bug` |
| a rule or doc is unclear and needs a decision | `question.yml` | `question` |
| the language should change | `language-change.yml` | `enhancement` |
| a pilot derivation is blocked (`sdtm_to_adam.md` section 4) | `bug-report.yml` | `pilot-dry-run` |
| a benchmark should be added or changed | the [benchmark factory](benchmark_factory/README.md) work item | |

## 2. What never goes public

- **Participant data.** Any record from a real study, even with identifiers
  replaced, dates shifted, or columns dropped. Combinations of values still
  identify people.
- **Study identity.** Sponsor, compound or product code, protocol number,
  real `STUDYID`, site or investigator names, unpublished indications, and
  study timelines.
- **Unpublished material.** Sponsor specifications, SAPs, mapping specs,
  TLF shells, results, and internal standards.
- **Environment.** Internal hostnames, URLs, repository names, file paths,
  user names, email addresses, and ticket numbers. Tracebacks and logs carry
  paths and user names; file names often carry study identifiers.
- **Secrets.** Tokens, passwords, keys, and connection strings.
- **Files that cannot be read in full.** Screenshots and binary attachments
  (`.parquet`, `.xpt`, `.xlsx`, `.pdf`) hide content and metadata. Paste
  text instead.

Public material is fine to cite: anything already in this repository (rules,
REQ IDs, `yaml/`, benchmarks, docs) and published CDISC standards and
controlled terminology.

## 3. Workflow

1. **Understand the finding privately.** Read the reviewer's material
   locally. Do not send any confidential string to another service while
   doing so: no web search, no paste into another tool. State the problem
   in one sentence in yamaa terms ("`previous_non_missing` returns the
   current row when ...").
2. **Anchor on public references.** Find the rule file, REQ ID, `yaml/`
   entry, doc page, or benchmark the finding is about. A rule or doc
   question often needs only the link and the two readings it allows; no
   data at all.
3. **Rebuild the example from scratch.** For an engine bug or a language
   change:
   - Prefer a public benchmark that shows the same behavior
     (`benchmarks/<name>/input/` and `spec.yaml`); cite its path.
   - Otherwise write a new minimal spec and invented input: only the
     columns and rows the failure needs, standard CDISC variable names, and
     obviously fake values (`STUDY01`, `STUDY01-001`, `2020-01-01`). Write
     the spec from the yamaa language, not by trimming the reviewer's spec.
   - Never make the example by editing real records or a real spec:
     renaming, shifting, subsetting, or masking is not enough.
   - Run it with the latest yamaa `main`, on both engines when the finding
     is about the engine, and confirm it still shows the problem. If it
     does not, the finding is not ready: return to step 1 or use section 4.
4. **Draft the issue.** Save the draft to a local file outside any
   repository that is pushed (`feedback-draft.md`). Follow the template's
   fields; title it `<area>: <what breaks>`. Paste output only from the
   synthetic run. Give the version as `pip show yamaa` output or a public
   commit hash, never an internal mirror URL.
5. **Check the draft for disclosure.** Run both passes over the whole draft:
   title, body, code blocks, file names, and links.
   - Mechanical scan. Ask the reviewer for their sensitive terms (sponsor,
     compound, protocol, study identifiers, internal domains) and keep them
     in a local file that is never committed or posted. Every hit must be
     removed or be a public link:

     ```bash
     grep -nFi -f private-terms.txt feedback-draft.md
     grep -nEi '[[:alnum:]._%+-]+@[[:alnum:].-]+\.[a-z]{2,}' feedback-draft.md
     grep -nEi '(/home|/Users|/mnt|/opt)/|[a-z]:\\|https?://' feedback-draft.md
     grep -nEi 'token|passw|secret|api[_-]?key|bearer' feedback-draft.md
     ```

   - Read-through. Every value in the draft came from step 2 or step 3. No
     value, name, or date came from the reviewer's material.
6. **Get the reviewer's approval.** Show the reviewer the exact final text
   and ask them to confirm that it is accurate and that their organization
   allows it to be public. Without an explicit yes, do not post. Then
   either the reviewer submits it through the issue form, or the agent
   posts it:

   ```bash
   gh issue create --repo elong0527/yamaa --title "<area>: <what breaks>" \
   	--body-file feedback-draft.md
   ```

   Delete the draft and the term list afterwards.
7. **Follow up the same way.** Every later comment, reply, attachment, or
   pull request repeats steps 3 to 6. When a maintainer asks for more
   evidence, answer with synthetic evidence only, on GitHub and by email
   alike.

## 4. When the evidence cannot be made public

- Do not post the evidence, in part or in summary.
- The reviewer takes it through their organization's approved internal
  process (the study owner, or the privacy or security contact).
- If the behavior can be described in general terms, file a public issue
  with the public references from step 2 and say that a public
  reproduction is not available yet. Maintainers may ask for a synthetic
  example there; never answer with private evidence.

## 5. If something confidential was posted

- Stop. Do not add to it, and do not quote it in a reply.
- Edit the text out, then delete the old revision from the comment's edit
  history; or delete the comment or issue. Notification emails already
  sent cannot be recalled.
- Removing an uploaded attachment from a comment does not remove the file;
  ask [GitHub Support](https://support.github.com/) to purge it.
- Revoke and rotate any exposed credential.
- Follow the reviewer's organization's incident process.
- Maintainers never copy the exposed content into tests, fixtures, commits,
  or CI artifacts; they ask for a synthetic reproduction instead.

## Checks before posting

- [ ] Public references from step 2 are cited.
- [ ] The example is a public benchmark or was written from scratch, and it
      reproduces with the latest yamaa `main`.
- [ ] Both scans in step 5 are clean, or each hit is a public link.
- [ ] No screenshots or binary attachments.
- [ ] The reviewer approved the exact final text.
