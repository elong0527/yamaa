import yamaa

# Named `out`, not `dm`: the test harness binds the run.py variable to the
# primary output's path stem, and this benchmark's output is `out.parquet`.
out = yamaa.yamaa_domain("spec.yaml").output
out
