# Conformance validation for derivation specifications.
#
# The R counterpart of tests/python/validate.py. Both implementations must
# accept and reject exactly the same specifications. NOTHING CHECKS THAT YET:
# there is no parity harness, and the two are known to disagree.

yaml_dir <- function() {
  normalizePath(file.path(dirname(sys.frame(1)$ofile %||% "."), "..", ".."),
                mustWork = FALSE)
}

`%||%` <- function(a, b) if (is.null(a)) b else a

YAML_DIR <- Sys.getenv("YAMAA_YAML_DIR", unset = normalizePath(
  file.path(dirname(normalizePath(sys.frame(1)$ofile %||% ".", mustWork = FALSE)), "..", ".."),
  mustWork = FALSE
))

load_design <- function(dir = YAML_DIR) {
  list(
    schema = yaml::read_yaml(file.path(dir, "schema.yaml")),
    ops    = yaml::read_yaml(file.path(dir, "operations.yaml"))$operations,
    exc    = yaml::read_yaml(file.path(dir, "exceptions.yaml"))$exceptions
  )
}

# R006 scalar resolution -----------------------------------------------------
# The R yaml package resolves bare Y/N/yes/no/on/off to logical. Every reader
# here must therefore force character, and specifications must not rely on it.

read_yaml_1_2 <- function(path) {
  yaml::read_yaml(path, handlers = list(
    bool_yes = function(x) x,
    bool_no  = function(x) x
  ))
}

scalar_resolution_errors <- function(path) {
  lines <- readLines(path, warn = FALSE)
  pat <- "^\\s*(-\\s*)?[A-Za-z_][A-Za-z0-9_.]*:\\s+(Y|N|yes|no|on|off|Yes|No|On|Off|YES|NO)\\s*$"
  hit <- grep(pat, lines)
  if (!length(hit)) return(character())
  sprintf("%s:%d: unquoted %s resolves as boolean in the R yaml package and/or PyYAML (R006)",
          path, hit, trimws(sub(".*:\\s+", "", lines[hit])))
}

fold_ascii <- function(s) {
  chartype <- strsplit(as.character(s), "")[[1]]
  paste(ifelse(chartype >= "A" & chartype <= "Z", tolower(chartype), chartype),
        collapse = "")
}

# R004 predicate identifiers. Kept deliberately simple: enough to check which
# output columns a predicate names, not a full parser.
predicate_vars <- function(text) {
  kw <- c("AND", "OR", "NOT", "IS", "NULL", "IN", "TRUE", "FALSE")
  stripped <- gsub("'([^']|'')*'", " ", text)
  toks <- regmatches(stripped, gregexpr("[A-Za-z_][A-Za-z0-9_.]*", stripped))[[1]]
  toks[!(toupper(toks) %in% kw)]
}


arg_spec <- function(entry) {
  out <- list()
  for (a in entry$arguments %||% list()) out[[names(a)[1]]] <- a[[1]]
  out
}

as_list_of <- function(x) {
  if (is.null(x)) return(list())
  if (!is.null(names(x))) list(x) else x
}

# R007/R008 registry invariants ----------------------------------------------

check_registries <- function(d = load_design()) {
  errs <- character()
  for (lbl in c("operation", "exception")) {
    reg <- if (lbl == "operation") d$ops else d$exc
    for (nm in names(reg)) {
      entry <- reg[[nm]]
      if (!nzchar(trimws(entry$semantics %||% ""))) {
        errs <- c(errs, sprintf("registry: %s '%s' has no semantics", lbl, nm))
      }
      for (arg in names(arg_spec(entry))) {
        desc <- arg_spec(entry)[[arg]]
        if (is.null(desc$default)) next
        if (isTRUE(desc$required)) {
          errs <- c(errs, sprintf("registry: %s.%s has a default on a required argument",
                                  nm, arg))
        }
        if (!is.null(desc$values) && !(desc$default %in% desc$values)) {
          errs <- c(errs, sprintf("registry: %s.%s default '%s' is not in [%s]",
                                  nm, arg, desc$default, paste(desc$values, collapse = ", ")))
        }
        ok <- switch(desc$type %||% "",
                     bool = is.logical(desc$default),
                     int = is.numeric(desc$default),
                     str = is.character(desc$default),
                     float = is.numeric(desc$default),
                     TRUE)
        if (!ok) {
          errs <- c(errs, sprintf("registry: %s.%s default does not satisfy type '%s'",
                                  nm, arg, desc$type))
        }
      }
    }
  }
  errs
}

# Specification validation ----------------------------------------------------

validate_spec <- function(path, d = load_design()) {
  errs <- scalar_resolution_errors(path)
  spec <- read_yaml_1_2(path)
  datasets <- names(spec$datasets %||% list())

  fail <- function(...) errs <<- c(errs, sprintf(...))

  closed <- function(val, cls, where) {
    allowed <- vapply(d$schema[[cls]], function(e) names(e)[1], "")
    for (k in names(val)) {
      if (!(k %in% allowed)) fail("%s: undeclared field '%s' for %s", where, k, cls)
    }
    for (e in d$schema[[cls]]) {
      nm <- names(e)[1]
      if (isTRUE(e[[1]]$required) && is.null(val[[nm]])) {
        fail("%s: missing required field '%s'", where, nm)
      }
    }
  }

  action <- function(act, where, reg, lbl) {
    if (length(act) != 1L) {
      fail("%s: action_class requires size 1", where); return(NULL)
    }
    nm <- names(act)[1]
    if (!(nm %in% names(reg))) {
      fail("%s: '%s' is not in the %s registry", where, nm, lbl); return(NULL)
    }
    entry <- reg[[nm]]
    args <- act[[1]] %||% list()
    spc <- arg_spec(entry)
    for (a in names(args)) {
      if (!(a %in% names(spc))) fail("%s.%s: unknown argument '%s'", where, nm, a)
    }
    for (a in names(spc)) {
      desc <- spc[[a]]
      if (isTRUE(desc$required) && is.null(args[[a]])) {
        fail("%s.%s: missing required argument '%s'", where, nm, a)
      }
      if (is.null(args[[a]])) next
      if (!is.null(desc$values) && !(args[[a]] %in% desc$values)) {
        fail("%s.%s.%s: '%s' is not one of [%s]", where, nm, a, args[[a]],
             paste(desc$values, collapse = ", "))
      }
      if (identical(desc$type, "dataset_id") && !(args[[a]] %in% datasets)) {
        fail("%s.%s.%s: '%s' is not a declared dataset", where, nm, a, args[[a]])
      }
    }
    if (identical(nm, "mapping") && identical(args$case_sensitive, FALSE)) {
      folded <- vapply(names(args$dict), fold_ascii, "")
      dup <- folded[duplicated(folded)]
      for (f in unique(dup)) {
        fail("%s.mapping.dict: keys fold to the same value under case_sensitive: false",
             where)
      }
    }
    entry
  }

  check_predicate <- function(text, where, label, declared) {
    for (n in predicate_vars(text)) {
      if (grepl(".", n, fixed = TRUE)) {
        fail("%s.%s: '%s' is qualified; a predicate over output rows names output columns (R001)",
             where, label, n)
      } else if (!(n %in% declared)) {
        fail("%s.%s: '%s' is not a declared output column (R001)", where, label, n)
      }
    }
  }

  derivation <- function(dv, where, driver) {
    closed(dv, "derivation_class", where)
    if (!is.null(dv$where)) check_predicate(dv$where, where, "where", declared)
    src <- dv$source
    if (!is.null(src) && grepl(".", src, fixed = TRUE)) {
      q <- sub("\\..*$", "", src)
      if (!(q %in% datasets)) {
        fail("%s.source: '%s' is not a declared dataset (R002)", where, q)
      }
    }
    joins <- !is.null(src) && grepl(".", src, fixed = TRUE) &&
      sub("\\..*$", "", src) != (driver %||% "")
    if (!is.null(dv$filter) && !joins) {
      fail("%s.filter: derivation performs no join (R003)", where)
    }

    ops <- as_list_of(dv$operations)
    entries <- list()
    for (i in seq_along(ops)) {
      e <- action(ops[[i]], sprintf("%s.operations[%d]", where, i - 1L), d$ops, "operation")
      entries[[i]] <- e
      if (!is.null(e) && identical(e$kind, "aggregate") &&
          !((i == 1L && joins) || !is.null(dv$group_by))) {
        fail("%s.operations[%d]: aggregate outside its two legal positions (R007)",
             where, i - 1L)
      }
    }
    seeded <- !is.null(dv$source) || !is.null(dv$literal)
    if (length(ops) && !is.null(entries[[1]]) &&
        identical(entries[[1]]$seed, "required") && !seeded) {
      fail("%s: leading operation declares seed: required but there is no source or literal (R007)",
           where)
    }
    if (!length(ops) && !seeded) fail("%s: no source, literal, or operations (R004)", where)

    stages <- character()
    exs <- as_list_of(dv$exception)
    for (i in seq_along(exs)) {
      e <- action(exs[[i]], sprintf("%s.exception[%d]", where, i - 1L), d$exc, "exception")
      if (is.null(e)) next
      nm <- names(exs[[i]])[1]
      if (e$stage %in% stages) {
        fail("%s.exception[%d].%s: stage '%s' already bound (R008)", where, i - 1L, nm, e$stage)
      }
      stages <- c(stages, e$stage)
      if (identical(e$stage, "operation")) {
        n <- sum(vapply(entries, function(x) !is.null(x) && nm %in% (x$raises %||% character()),
                        logical(1)))
        if (n != 1L) {
          fail("%s.exception[%d].%s: bound to %d operations that raise it, requires 1 (R008)",
               where, i - 1L, nm, n)
        }
      }
    }
  }

  declared <- vapply(spec$columns, function(c) c$name, "")
  closed(spec, "root_class", "root")
  if (!is.null(spec$domain) && spec$domain %in% datasets) {
    fail("root.datasets: '%s' is also the output domain (R002)", spec$domain)
  }
  rows <- spec$rows %||% list()
  if (!length(rows) && is.null(spec$base)) fail("root: no rows entry and no base (R001)")
  if (!is.null(spec$base) && !(spec$base %in% datasets)) {
    fail("root.base: '%s' is not a declared dataset (R002)", spec$base)
  }

  for (col in spec$columns) {
    closed(col, "column_class", sprintf("columns[%s]", col$name))
    if (!is.null(col$derivation)) {
      derivation(col$derivation, sprintf("columns[%s].derivation", col$name), spec$base)
    }
  }
  for (r in rows) {
    closed(r, "row_class", sprintf("rows[%s]", r$id))
    if (!is.null(r$dataset) && !(r$dataset %in% datasets)) {
      fail("rows[%s].dataset: '%s' is not declared (R002)", r$id, r$dataset)
    }
    for (nm in names(r$derivations)) {
      if (!(nm %in% declared)) fail("rows[%s]: target '%s' is not a declared column", r$id, nm)
      derivation(r$derivations[[nm]], sprintf("rows[%s].%s", r$id, nm),
                 r$dataset %||% spec$base)
    }
  }

  col_derived <- vapply(spec$columns, function(c) !is.null(c$derivation), logical(1))
  col_derived <- declared[col_derived]
  row_targets <- lapply(rows, function(r) names(r$derivations))
  for (nm in declared) {
    covered <- nm %in% col_derived ||
      (length(row_targets) && all(vapply(row_targets, function(t) nm %in% t, logical(1))))
    if (!covered) fail("R005 coverage: '%s' has no column derivation and is missing from a rows entry", nm)
  }
  for (k in spec$keys %||% character()) {
    if (!(k %in% declared)) fail("R005: key '%s' is not a declared column", k)
  }

  errs
}

example_specs <- function(dir = YAML_DIR) {
  sort(Sys.glob(file.path(dir, "examples", "*", "spec.yaml")))
}
