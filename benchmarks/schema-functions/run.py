import yamaa
from yamaa.functions import run_with_project_functions

test = run_with_project_functions("spec.yaml", project_root="python").output
test
