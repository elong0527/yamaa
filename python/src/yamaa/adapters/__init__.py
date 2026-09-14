"""Adapters that carry real engine results to a protocol owned elsewhere.

An adapter reads what the engine already produced and says it in another
vocabulary. It decides no semantics of its own: an adapter that disagrees
with the engine is the adapter's bug.

Import each adapter by name -- `yamaa.adapters.conformance` -- so that
running one as `python -m` executes the module once.
"""
