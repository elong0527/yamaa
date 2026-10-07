# CSV profile failure truth

Thirteen independently authored byte containers were run through the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. The specification is the same for every case. `source_hex` retains exact bytes, including invalid UTF-8 and CR. Complete reference reports pin source read observations, ingestion timing, condition/requirement, paths and typed context. Only runtime/backend identity is fixed to the existing native replay convention.

All ten closed R023 CSV failure causes are exercised. Decode precedes BOM/syntax, complete syntax precedes header/width checks, and quoted-prefix UTF-8 coordinates retain record/field identity. The parser algorithm and existing collected-text fixtures are preserved. Resource limits remain boundary policy failures without fabricated language diagnostics.

Rust and both installed hosts replay all complete reports, then drop the prepared specification and verify owned observations and repeated failed-save gates without recapture or publication. No existing benchmark expectation is changed.
