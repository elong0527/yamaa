# yamaa bounded 100k frontier probe

This isolated candidate is unapproved. It makes the approved workload sizes
testable; neither these ceilings nor a cutover or timing budget is approved.
The previous immutable-row and cumulative-report records retain their original
source revisions and smaller policy scopes.

Ordinary source admission is capped at 524,288 rows, 2,097,152 cells and 16 MiB
of cumulative captured CSV bytes. Each CSV keeps its 8 MiB and 1,048,576-field
limit. Ordinary execution admits at most 134,217,728 logical WorkCells,
131,072 output rows, 1,048,576 output cells, 524,288 key cells and 2 MiB of
output text. Identity admission remains 65,536 cells and 1 MiB of text;
scalar text and document, per-file, IPC and private host metadata limits remain
separately bounded.

The actual authored core compiler's 100k lookup with multiplicity four needs
442,856 source rows, 1,128,568 source cells, 8,647,622 cumulative CSV bytes,
126,950,430 WorkCells and 1,547,619 output-text bytes. Its donor alone is
7,847,619 bytes with 1,028,568 data fields plus three headers. The engine
component equality and one-under records are independent of public report,
host-copy and Save admission; the latter still require installed witnesses.

Graph source rows retain the separate 1,048,576 cumulative ceiling. The
reviewed shared-work implementation accounts actual work across nodes and
reserves source rows by declared input slots. Output cells reserve a separate
2,097,152-cell total by actual compiled owned column counts before activation.
For depth three, widths [2,2,2,2,2,4] reserve 299,593 cells for each leaf and
599,186 for the root, with their sum below the total. Zero-column plans retain
zero output-cell admission. Other graph text and identity shares remain
unchanged pending an actual refusal and bounded allocation review.

The initial report probe has independent ceilings: 160 MiB encoded JSON,
768 MiB of admitted retained projection storage, 4,194,304 traversed report
cells and 536,870,912 logical report-work units. These are enforced admission
units, not RSS guarantees. Native node reports share actual cumulative encoded
length and retained charges. Complete aggregate reports and every possible
publication prefix are separately admitted before the first publisher.

The authored 100k depth-three geometry provides a conservative initial probe,
not a substitute for measurement. Twelve two-column row arrays need at most
7,900,001 bytes each and the root four-column array needs 16,700,001 bytes:
111,500,013 bytes altogether. Their geometric requested capacities sum to at
most 112 MiB, charged at three times capacity for buffer/boxing/Arc coexistence.
All six expected CSVs together contain 8,040,054 bytes and 600,006 newlines.
Allowing four content occurrences and eight escaped-newline bytes per newline,
plus an exploratory 8 MiB metadata/prefix reserve, gives 156,848,885 encoded
bytes. The reserve is a capped probe assumption; actual complete prefixes must
replace it before approval. Cumulative node artifact content and record-string
admission contributes at most 269,762,064 retained charge bytes; together with
the row-buffer charge this is 622,083,600 bytes before smaller metadata.

The opt-in capacity trace records actual encoded row lengths and requested
buffer capacities at the original encoding boundary. Installed full-output,
empty-issues, Build-no-publication and two-held-Save checks will retain actual
report lengths, each admission scope's charges and exact-child peak RSS.
Shared-host frontier observations are diagnostic and cannot approve release
timing distributions or same-machine numerical budgets.

Boundary controls preserve complete accepted graph ownership when the public
typed copy exceeds 4,194,304 cumulative cells. A borrowed oversized report
identity is refused before any formatter ownership growth even when the caller
supplies usize::MAX. Publication error inventories, distinct request keys and
node identities, all complete prefixes, original failures and retries remain
covered by their existing independent controls.
