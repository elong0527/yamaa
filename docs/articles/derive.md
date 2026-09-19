# KRC: the yamaa derivation model

The principles page gave you the contract: one execution — `data_output = derive(data_input, spec)` — with a closed vocabulary for what you can write (spec) and fixed semantics for what it means (derive). KRC is the derive half: the three-step model of what the engine does with your spec.

**Key the rows, section the rows, derive the columns.**

## Key — decide which rows exist

Key expressions run over source rows and produce one row per distinct key combination. Then the row set freezes: nothing downstream can modify keys or add/remove rows.

## Section — organize the rows

Sections group rows for scoped derivation: window groups, union branches, per-section logic. Sections are implied by how rows are organized — ordering, union branches, windows — not declared separately. Declaring rows is sufficient.

Union is implicit: a key-union plus an implicit branch key, with branch-specific behavior as section logic.

## Derive — append one column per pass

Each column is one pass over the frozen rows, each declared column derived in exactly one place — the Explicit principle, as the engine sees it. For each row, find candidate source records by key equality plus an optional predicate; the matched set reduces to exactly one scalar. That reduction is where "where it would have two, YAMAA fails instead of choosing" is implemented. Evaluation is column-major — fully materialize one column before deriving its dependents — and dependencies form a DAG.

Two shapes of derivation:

- **Aggregate** (many-to-one): reduce a row's matched source records to one scalar — `first`, `count`, `min`, `max`, `sum`.
- **Window** (many-to-many): derive one value per row from ordered sibling rows in its section. Prefix/positional only.

A missing policy (`missing:`/`strict:`) is a per-column annotation, not a manipulation.

## In practice

A DM derivation: key expressions over source rows yield one row per `(STUDYID, USUBJID)` and freeze. Then one pass per column — `SEX` matches each row's source records by key equality plus a predicate and reduces to one scalar; `AGE` computes over already-materialized columns. A window ordered by visit date within each subject lets February's missing weight read January's materialized weight.

## Vocabulary

The working terms of derive: key, row, column, Key, Derive, Section, matched set, pass, window, partition, missing policy, root, branch.

- **Partition**: window-speak for section — the row group a window runs over.
- **Key expressions** see source rows only; cross-dataset derivation composes specifications before derivation runs.
- **Ordering** is declared at two levels: root ordering for output sequencing, section ordering for row-relative derivation.
- **`function`** is the escape hatch for specialized reductions: Derive with an opaque body.

## Next

KRC is what the engine does. For what you write — the spec half of the contract — read Schema concepts.
