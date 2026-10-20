# Installed project programs

The five function studies share ordinary installed Python and R packages.
Each study's `project.yaml` links its Python and R environment files, whose
function metadata is explicit; there is no artifact loader or separate
contract/conformance format. Every positive expected CSV is the original truth.
The negative study independently specifies a missing required argument (FUNC-5).

From the repository root, install the Python package and its project programs:

```sh
uv sync --project python --locked --group benchmark --extra test --no-editable
python/.venv/bin/python -m tools.staging.stage_project_inputs /tmp/yamaa-project-studies
cd /tmp/yamaa-project-studies/schema-functions
/path/to/yamaa/python/.venv/bin/python - <<'PYTHON'
import yamaa
run = yamaa.project("project.yaml").spec("TEST").run()
run.save()
PYTHON
```

The stager copies the exact existing `python/uv.lock` beside the Python
environment file each isolated study's project links. It refuses an existing
destination. No second Python lock is committed.

For R, install `rust/tests/project-programs/r` into the same library as the standalone
`yamaa` package and open the study's specification through its project. Each R
environment refers to its hash-free `renv.lock`, which pins `yamaa` and the
called project package.

```r
install.packages("rust/tests/project-programs/r", repos = NULL, type = "source")
spec <- yamaa::yamaa_project("project.yaml")$spec("TEST")
spec$check()$issues
run <- spec$run()
run$save()
```

CI runs the complete installed public environment suite in Python wheel/source
lanes and R source lanes. The historical reference-assisted inventory keeps its
other gates; its three function-specific required rows move to the shared public
suite, where all five studies run against both hosts and independent truth.
