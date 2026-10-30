# Benchmark document preview fixtures

`test_generate.py` reads the case tables in this directory. The HTML cases
stage `report.html` under each declared suffix and compare the complete preview
record with `html-previews.json`; the script is deliberately inert in the
isolated frame. `transport-previews.json` checks the empty preview for an
authored uppercase transport filename. `define-summaries.json` names existing
authored benchmark XML documents and their dataset summaries, including resolved dataset comments and
document references. Expectations are authored from those inputs, never captured
from the renderer.
