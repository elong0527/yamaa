library(yamaa)

# Named `adsl`, not `dm`: the test harness binds the run.py variable to the
# primary output's path stem, and this benchmark's output is `adsl.parquet`.
adsl <- yamaa_domain("spec.yaml")$output
adsl
