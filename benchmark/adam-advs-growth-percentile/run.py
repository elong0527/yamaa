import yamaa
from yamaa.functions import run_with_project_functions

advs = run_with_project_functions("spec.yaml", project_root="python").output
advs
