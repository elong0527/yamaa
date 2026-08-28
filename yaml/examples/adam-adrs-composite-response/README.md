# ADaM ADRS composite responder endpoint

This fixture answers one question: can a response derived from an efficacy
threshold, a safety condition, and a discontinuation rule be expressed as one
analysis value?

## Rule and input boundary

The input is a pre-derived ADRS slice carrying the percentage change in EASI at
week four. ADSL contributes a serious-adverse-event flag and a discontinuation
reason by `STUDYID` and `USUBJID`.

The endpoint has three components and a fixed precedence:

1. a subject with a serious adverse event or any discontinuation reason is a
   non-responder regardless of the efficacy value;
2. a subject with no efficacy value is not evaluable;
3. a subject whose percentage change is at most `-75` is a responder;
4. everyone else is a non-responder.

The five subjects cover a responder, a non-responder below threshold, a subject
who meets the threshold but is overridden by safety, a subject with no efficacy
value who discontinued, and a subject with no efficacy value and no rule to
apply.

## Precedence is branch order, and the reason must be derived twice

`case` evaluates branches in order, so the whole endpoint is one expression and
the precedence is real. Subject `CATH-UCSD-0003` has a percentage change of
`-85` and is still a non-responder, and subject `CATH-UCSD-0004` is a
non-responder rather than not evaluable because the discontinuation rule
outranks the missing component.

The reason a subject received their value is not recoverable from `AVALC`.
Three different paths produce `NON-RESPONDER`. `ARSN` records which one
applied, and it does so by repeating the same four predicates in the same order
in a second `case`. Nothing links the two derivations, and an edit to one
silently desynchronizes the endpoint from its own audit trail.

That is the sharpest finding here. A derivation whose branches carry both a
value and a reason would remove the duplication; so would a named intermediate
holding the matched branch.

## Three further gaps

1. **The missing-component policy is implicit.** Whether a missing component
   means not evaluable or non-response is expressed only by where the branch
   sits in the list. No declaration states the policy, so two studies cannot be
   compared without reading the branch order.
2. **The threshold is a literal.** `-75` appears inside a predicate and cannot
   be read from a parameter dataset, the same constraint recorded by
   `../adam-adlb-closest-visit`.
3. **Visit selection is out of scope here.** The input is already one record
   per subject at one visit. Choosing which visit feeds a responder definition
   needs the window and selection machinery exercised by
   `../adam-adlb-closest-visit`, which cannot be combined with this derivation
   in one specification without emitting both sets of intermediates.

## Diagnostics and verifications

No handler path is declared. `SAEFL` and `DCSREAS` reach every row through the
R003 join, and every subject has an ADSL record.

Rows remain in source order; the key is
`[STUDYID, USUBJID, PARAMCD, AVISIT]`; exactly five rows are expected. A
missing numeric response must mean not evaluable, a responder must meet the
threshold, and any safety or discontinuation condition must produce a
non-responder.
