# functions.R -- project function bindings runtime (rules/operations/functions.md).
#
# Stage 2. A spec calling `function:` resolves each logical function name
# through the project environment: environment.yaml at the project root
# declares the runtime language, the per-function contracts, and the
# binding.call for this runner's language. The R runner's project root is
# the spec directory; a benchmark shipping only a python/ project is
# declined with runner_language_mismatch (REQ-0667), not an engine gap.
#
# Validation order is load-bearing: contract checks (REQ-0698/0699) run
# before the language gate (REQ-0696), because the environment declares
# contracts language-neutrally (see negative-function-contract).

# collect every `function:` call in the (resolved) spec: name + contract_version
collect_function_calls <- function(x) {
  calls <- list()
  walk <- function(x) {
    if (is.list(x)) {
      nm <- names(x)
      if (!is.null(nm) && "function" %in% nm) {
        p <- x[["function"]]
        calls[[length(calls) + 1]] <<- list(name = p$name,
          contract_version = p$contract_version)
      }
      for (el in x) walk(el)
    }
  }
  walk(x)
  calls
}

# the project root is the first directory holding environment.yaml: the
# project directory (read only to decline it honestly under REQ-0667)
find_project_root <- function(spec_dir) {
  for (d in c(spec_dir, file.path(spec_dir, "python"))) {
    if (file.exists(file.path(d, "environment.yaml"))) return(d)
  }
  NULL
}

# read + shape-check the project environment (REQ-0694/0695)
load_project_environment <- function(spec_dir) {
  root <- find_project_root(spec_dir)
  if (is.null(root))
    yamaa_error("project_environment_missing",
      "no environment.yaml at the project root")
  env <- tryCatch(yaml_load_file(file.path(root, "environment.yaml")),
    error = function(e) yamaa_error("project_environment_invalid",
      paste0("cannot parse environment.yaml: ", conditionMessage(e))))
  if (is.null(env$schema_version) || is.null(env$version) ||
      is.null(env$runtime$language) ||
      is.null(env$functions) || length(env$functions) == 0)
    yamaa_error("project_environment_invalid",
      "environment.yaml misses schema_version, version, runtime.language, or functions")
  list(env = env, root = root)
}

# the exact contract version the environment offers for a logical function:
# function name (project-root-local, REQ-1069)
function_contract_version <- function(entry, name, root) {
  # NB: exact [[ matching -- `$` would partial-match `contract` to
  # `contract_version`
  has_ref <- !is.null(entry[["contract"]])
  has_inline <- !is.null(entry[["contract_version"]])
  if (has_ref && has_inline)
    yamaa_error("project_environment_invalid",
      paste0("function '", name, "' declares its contract both inline and by reference"))
  if (has_ref) {
    doc <- tryCatch(yaml_load_file(file.path(root, entry[["contract"]])),
      error = function(e) yamaa_error("project_environment_invalid",
        paste0("cannot read shared contract '", entry[["contract"]], "'")))
    centry <- doc[[name]]
    if (is.null(centry) || is.null(centry[["contract_version"]]))
      yamaa_error("project_environment_invalid",
        paste0("shared contract '", entry[["contract"]], "' does not define '", name, "'"))
    return(as.character(centry[["contract_version"]]))
  }
  if (has_inline) return(as.character(entry[["contract_version"]]))
  yamaa_error("project_environment_invalid",
    paste0("function '", name, "' declares no contract"))
}

# REQ-0667/0694/0696/0698/0699: validate every function call against the
check_function_runtime <- function(spec, spec_dir) {
  calls <- collect_function_calls(spec)
  if (length(calls) == 0) return(NULL)
  pe <- load_project_environment(spec_dir)
  env <- pe$env; root <- pe$root
  for (call in calls) {
    entry <- env$functions[[call$name]]
    if (is.null(entry))
      yamaa_error("unknown_project_function",
        paste0("no declared project function: ", call$name))
    avail <- function_contract_version(entry, call$name, root)
    if (is.null(call$contract_version) ||
        !identical(as.character(call$contract_version), avail))
      yamaa_error("function_contract_mismatch",
        paste0("function '", call$name, "' contract version '",
          call$contract_version, "' unavailable (environment offers '", avail, "')"))
  }
  if (!identical(as.character(env$runtime$language), "r"))
    yamaa_error("runner_language_mismatch",
      paste0("project runtime language is '", env$runtime$language,
        "'; this R runner supports only 'r'"))
  pe
}

# resolve a binding.call to an R callable. `pkg::fun` first consults the
resolve_project_callable <- function(call_str, project_fns) {
  parts <- strsplit(call_str, "::", fixed = TRUE)[[1]]
  if (length(parts) == 2) {
    pkg <- parts[1]; fun <- parts[2]
    if (exists(fun, envir = project_fns, inherits = FALSE))
      return(get(fun, envir = project_fns, inherits = FALSE))
    ns <- tryCatch(getNamespace(pkg), error = function(e) NULL)
    if (!is.null(ns) && exists(fun, envir = ns, inherits = FALSE))
      return(get(fun, envir = ns, inherits = FALSE))
    yamaa_error("unknown_project_function",
      paste0("unresolvable binding: ", call_str))
  }
  if (exists(call_str, envir = project_fns, inherits = FALSE))
    return(get(call_str, envir = project_fns, inherits = FALSE))
  yamaa_error("unknown_project_function",
    paste0("unresolvable binding: ", call_str))
}

# the effective contract fields for a logical function: the shared contract
# document when the entry references one, else the inline fields
function_contract_fields <- function(pe, name) {
  entry <- pe$env$functions[[name]]
  ref <- entry[["contract"]]  # exact [[ : `$` would partial-match
  if (is.null(ref)) return(entry)
  doc <- tryCatch(yaml_load_file(file.path(pe$root, ref)),
    error = function(e) yamaa_error("project_environment_invalid",
      paste0("cannot read shared contract '", ref, "'")))
  centry <- doc[[name]]
  if (is.null(centry))
    yamaa_error("project_environment_invalid",
      paste0("shared contract '", ref, "' does not define '", name, "'"))
  centry
}
