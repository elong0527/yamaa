# Installed project programs

The five function studies share ordinary installed Python and R packages.
Their environment metadata is explicit; there is no artifact loader or separate
contract/conformance format. Every positive expected CSV is the original truth.
The negative study independently specifies a missing required argument (REQ-0700).

From the repository root, install the Python package and its project programs:

```sh
uv sync --project python --locked --group benchmark --extra test --no-editable
python/.venv/bin/python rust/tools/stage_project_inputs.py /tmp/yamaa-project-studies
cd /tmp/yamaa-project-studies/schema-functions
/path/to/yamaa/python/.venv/bin/python run.py
```

The stager copies the exact existing `python/uv.lock` into each isolated study.
It refuses an existing destination. No second Python lock is committed.

For R, install `rust/tests/project-programs/r` into the same library as the standalone
`yamaa` package and run the staged study's `run.R`. Each R environment refers to
its hash-free `renv.lock`, which pins `yamaa` and the called project package.

```r
install.packages("rust/tests/project-programs/r", repos = NULL, type = "source")
yamaa::yamaa_check("spec.yaml", environment = "r/environment.yaml")
result <- yamaa::yamaa_domain("spec.yaml", environment = "r/environment.yaml")
result$save()
```

CI runs the complete installed public environment suite in Python wheel/source
lanes and R source lanes. The historical reference-assisted inventory keeps its
other gates; its three function-specific required rows move to the shared public
suite, where all five studies run against both hosts and independent truth.
