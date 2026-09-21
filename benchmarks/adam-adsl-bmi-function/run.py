import yamaa
from yamaa.functions import run_with_project_functions

adsl = run_with_project_functions("spec.yaml", project_root="python").output
adsl
