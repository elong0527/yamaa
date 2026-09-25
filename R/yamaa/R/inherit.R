# inherit.R -- spec composition: the `parents:` layer resolver.
#
# resolve_spec(entry_path) loads one spec and composes its inherited layers
# into a single resolved spec (rules/specification/composition.md,
# REQ-0614..REQ-0661). Pipeline: normalize refs -> DFS traversal (cycle
# detection) -> version/output checks -> canonicalize layers -> accumulate
# root and keyed members -> rebase paths -> verification defaults ->
# prune dead members -> deterministic column order -> schema field order.

INHERIT_SCHEMA_VERSION <- "1.0"

# ---- entry point ---------------------------------------------------------
resolve_spec <- function(entry_path) {
  entry_path <- normalizePath(entry_path, mustWork = TRUE)
  entry <- yaml_load_file(entry_path)
  if (is.null(entry$parents)) return(entry)
  layers <- collect_layers(entry_path, entry)
  check_layer_versions(layers)
  entry_dir <- dirname(entry_path)
  acc <- list()
  for (ly in layers) {
    frag <- rebase_layer_paths(canon_layer(ly$spec), ly$dir, entry_dir)
    acc <- compose_root(acc, frag)
  }
  acc <- materialize_verif_defaults(acc)
  acc <- prune_composed(acc)
  acc <- order_composed_columns(acc)
  acc <- finalize_composed(acc)
  # REQ-0657: the entry may inherit output (REQ-0623); only a resolved
  # spec to which no layer contributed output fails, as
  # missing_required_field.
  if (is.null(acc$output))
    yamaa_error("missing_required_field", "no layer contributed output")
  acc
}

# ---- parent reference validation and traversal ---------------------------
# normalize_parents(parents, decl_path): validate the parent references
# REQ-0615/0617/0618/0619/0653/0654.
normalize_parents <- function(parents, decl_path) {
  if (is.null(parents)) return(character(0))
  if (is.character(parents)) parents <- as.list(parents)  # REQ-0617: one path
  decl_dir <- dirname(decl_path)
  vapply(parents, function(p) resolve_parent_ref(p, decl_dir, decl_path),
    character(1))
}

# resolve_parent_ref(p, decl_dir, decl_path): one parent reference -> one
# canonical identity. REQ-0618: absolute local paths are permitted; remote
# (URI) references fail with invalid_parent_path (REQ-0653).
resolve_parent_ref <- function(p, decl_dir, decl_path) {
  bad <- !is.character(p) || length(p) != 1 || !nzchar(p) ||
    grepl("^[a-zA-Z][a-zA-Z0-9+.-]*:", p)  # any URI scheme, incl. file://
  if (bad)
    yamaa_error("invalid_parent_path",
      paste0("parent reference is not a local path: ", deparse1(p),
        " (declared in ", decl_path, ")"))
  full <- if (grepl("^/", p)) p else file.path(decl_dir, p)
  if (!file.exists(full) || dir.exists(full))
    yamaa_error("parent_not_found",
      paste0("parent file not found: ", p, " (declared in ", decl_path, ")"))
  normalizePath(full, mustWork = TRUE)  # REQ-0619: canonical identity
}

# collect_layers(entry_path, entry): ordered list of layers, each
# list(path, dir, spec), in contribution order. REQ-0620/0621: each layer's
# is a cycle and fails with inheritance_cycle (REQ-0655).
collect_layers <- function(entry_path, entry) {
  specs <- list()
  specs[[entry_path]] <- entry
  order <- character(0)
  active <- c(entry_path)
  visit <- function(path) {
    if (path %in% active)
      yamaa_error("inheritance_cycle",
        paste0("parent chain returns to a layer on the active path: ", path))
    if (path %in% names(specs)) return()  # contribution complete: skip
    specs[[path]] <<- yaml_load_file(path)
    dir <- dirname(path)
    ps <- normalize_parents(specs[[path]]$parents, path)
    active <<- c(active, path)
    for (p in ps) visit(p)
    active <<- active[-length(active)]
    order <<- c(order, path)
  }
  for (p in normalize_parents(entry$parents, entry_path)) visit(p)
  order <- c(order, entry_path)
  lapply(order, function(p) list(path = p, dir = dirname(p), spec = specs[[p]]))
}

# check_layer_versions(layers): every layer declares the same schema
# version as the bundle. REQ-0656: a missing or inconsistent layer version
check_layer_versions <- function(layers) {
  bad <- Filter(function(ly) {
    v <- ly$spec$schema_version
    is.null(v) || length(v) != 1 || v != INHERIT_SCHEMA_VERSION
  }, layers)
  if (length(bad) > 0)
    yamaa_error("schema_version_mismatch",
      paste0("layer schema_version is missing or not ", INHERIT_SCHEMA_VERSION,
        ": ", paste(vapply(bad, function(ly) {
          paste0(ly$path, " (", deparse1(ly$spec$schema_version), ")")
        }, character(1)), collapse = ", ")))
}

# ---- layer canonicalization ----------------------------------------------
# canon_layer(spec): expand per-layer shorthands before composition
# (REQ-0626) and drop the parents field.
canon_layer <- function(spec) {
  out <- spec
  out$parents <- NULL
  if (!is.null(out$input)) {
    for (id in names(out$input)) {
      v <- out$input[[id]]
      if (is.character(v)) out$input[[id]] <- list(path = v)  # REQ-0631
    }
  }
  out$verifications <- canon_verif_list(out$verifications)
  if (!is.null(out$columns))
    out$columns <- lapply(out$columns, canon_column)
  if (!is.null(out$rows))
    out$rows <- lapply(out$rows, function(r) {
      if (!is.null(r$derivations))
        r$derivations <- lapply(r$derivations, canon_derivation)
      r
    })
  out
}

canon_column <- function(c) {
  c$derivation <- canon_derivation(c$derivation)
  c$verifications <- canon_verif_list(c$verifications)
  c
}

# canon_verif_list(v): expand the single-verification shorthand to a
# one-element list (REQ-0265).
canon_verif_list <- function(v) {
  if (is.null(v)) return(NULL)
  nm <- names(v)
  if (!is.null(nm) && any(nzchar(nm))) list(v) else v
}

# canon_derivation(d): canonicalize one derivation. A bare string becomes
# {value: ...} wrapper unwraps any bare string it holds (REQ-0626).
canon_derivation <- function(d) {
  if (is.character(d) && length(d) == 1) return(list(value = canon_expr(d)))
  if (is.list(d) && !is.null(names(d))) {
    if ("value" %in% names(d)) {
      d$value <- canon_expr(d$value)
      return(d)
    }
    return(list(value = canon_expr(d)))
  }
  d
}

# canon_expr(x): expand a bare string to {source: {variable: x}} and a
canon_expr <- function(x) {
  if (is.character(x) && length(x) == 1)
    return(list(source = list(variable = x)))
  if (!is.list(x) || is.null(names(x))) return(x)
  k <- names(x)[1]
  if (k == "source") x[[1]] <- canon_source_payload(x[[1]])
  else if (k == "case")
    x[[1]] <- lapply(x[[1]], function(br) {
      br$then <- canon_expr(br$then)
      if (!is.null(br$otherwise)) br$otherwise <- canon_expr(br$otherwise)
      br
    })
  else if (k == "value") x[[1]] <- canon_expr(x[[1]])
  x
}

canon_source_payload <- function(p) {
  if (is.character(p) && length(p) == 1) list(variable = p) else p
}

# ---- path rebasing -------------------------------------------------------
# rebase_layer_paths(spec, from_dir, entry_dir): every contributed path-typed
# value is rebased relative to the entry file (REQ-0635/0636). Relative
# sees (REQ-0637).
rebase_layer_paths <- function(spec, from_dir, entry_dir) {
  rebase <- function(p) {
    if (!is.character(p) || length(p) != 1 || !nzchar(p) ||
        grepl("^/", p)) return(p)
    rel <- file.path(from_dir, p)
    if (identical(normalizePath(from_dir), normalizePath(entry_dir))) return(p)
    fp <- normalizePath(rel, mustWork = FALSE)
    ep <- normalizePath(entry_dir, mustWork = TRUE)
    out <- sub(paste0("^", gsub("([][{}()+*^$|\\\\?.])", "\\\\\\1", ep), "/?"),
      "", fp)
    if (identical(out, fp)) return(p)  # outside the entry tree: keep literal
    attr(out, "yamaa_orig") <- p
    out
  }
  if (!is.null(spec$input))
    for (id in names(spec$input)) {
      v <- spec$input[[id]]
      if (is.list(v) && !is.null(v$path)) v$path <- rebase(v$path)
      spec$input[[id]] <- v
    }
  if (!is.null(spec$output)) {
    if (!is.null(spec$output$path))
      spec$output$path <- rebase(spec$output$path)
    if (!is.null(spec$output$warning_log))
      spec$output$warning_log <- rebase(spec$output$warning_log)
    if (!is.null(spec$output$verification_log))
      spec$output$verification_log <- rebase(spec$output$verification_log)
  }
  spec
}

# ---- root and member composition ------------------------------------------
# compose_root(acc, frag): fold one canonicalized layer into the
# accumulated object. REQ-0627..REQ-0629: null clears an immediate field,
compose_root <- function(acc, frag) {
  for (f in names(frag)) {
    v <- frag[[f]]
    if (is.null(v)) {
      # REQ-0632: null clears an inherited optional field at the root
      # boundary; clearing with no inherited value is invalid.
      if (f %in% names(acc)) acc[[f]] <- NULL
      else yamaa_error("invalid_clear",
        paste0("null clears a field with no inherited value: ", f))
      next
    }
    if (f %in% c("input", "intermediates", "columns", "rows"))
      acc[[f]] <- compose_keyed(acc[[f]], v, f)
    else acc[[f]] <- v
  }
  acc
}

# keyed_id(m, kind, key): the identity of a keyed-collection member.
# REQ-0624: the mapping key identifies an input member; id identifies an
keyed_id <- function(m, kind, key = NULL) {
  if (kind == "input") return(key)
  fld <- switch(kind, intermediates = "id", columns = "name", rows = "id")
  m[[fld]]
}

# compose_keyed(acc, frag, kind): left-to-right member composition.
# member composes with its inherited declaration (REQ-0624).
compose_keyed <- function(acc, frag, kind) {
  if (is.null(acc)) acc <- list()
  if (kind == "input") {
    ids <- names(acc)
    for (key in names(frag)) {
      j <- match(key, ids)
      if (is.na(j)) {
        acc <- c(acc, setNames(list(frag[[key]]), key))
        ids <- c(ids, key)
      } else acc[[j]] <- compose_input_member(acc[[j]], frag[[key]])
    }
    return(acc)
  }
  ids <- vapply(acc, function(m) keyed_id(m, kind), character(1))
  for (m in frag) {
    id <- keyed_id(m, kind)
    j <- match(id, ids)
    if (is.na(j)) {
      acc <- c(acc, list(m))
      ids <- c(ids, id)
    } else acc[[j]] <- compose_member(acc[[j]], m, kind)
  }
  acc
}

# compose_input_member(a, d): merges the immediate fields of the member;
# a present non-null member field replaces its complete value (REQ-0623).
compose_input_member <- function(a, d) {
  out <- a
  for (f in names(d)) {
    v <- d[[f]]
    if (is.null(v)) {
      if (f %in% names(out)) out[[f]] <- NULL
      else yamaa_error("invalid_clear",
        paste0("null clears a field with no inherited value: input.", f))
      next
    }
    out[[f]] <- v
  }
  out
}

# compose_member(a, d, kind): intermediate and row members merge immediate
# fields like inputs (REQ-0623); column members compose field by field by
# kind (REQ-0628).
compose_member <- function(a, d, kind) {
  if (kind == "columns") return(compose_column(a, d))
  compose_input_member(a, d)
}

# compose_column(a, d): a written scalar or list replaces; mapping/dict
# written value of a different kind replaces what it inherits (REQ-0628).
compose_column <- function(a, d) {
  out <- a
  for (f in names(d)) {
    v <- d[[f]]
    if (is.null(v)) {
      if (f %in% names(out)) out[[f]] <- NULL
      else yamaa_error("invalid_clear",
        paste0("null clears a field with no inherited value: column.", f))
      next
    }
    if (!f %in% names(out)) {
      out[[f]] <- v  # a field the member does not carry is taken whole
      next
    }
    out[[f]] <- if (f == "derivation") compose_derivation(out[[f]], v)
    else if (f %in% c("metadata", "submission") &&
             both_named_lists(out[[f]], v)) merge_named(out[[f]], v)
    else v
  }
  out
}

both_named_lists <- function(a, b) {
  is.list(a) && is.list(b) && !is.null(names(a)) && !is.null(names(b)) &&
    any(nzchar(names(a))) && any(nzchar(names(b)))
}

# assign_keep_null(out, k, v): out[[k]] <- v, but an explicit null keeps
# the key (R's [[<- with NULL deletes; yaml keeps explicit nulls).
assign_keep_null <- function(out, k, v) {
  if (is.null(v)) out[k] <- list(NULL) else out[[k]] <- v
  out
}

merge_named <- function(a, b) {
  out <- a
  for (k in names(b))
    out <- assign_keep_null(out, k,
      if (k %in% names(a) && both_named_lists(a[[k]], b[[k]]))
        merge_named(a[[k]], b[[k]]) else b[[k]])
  out
}

# compose_derivation(a, d): both derivations are canonicalized to
compose_derivation <- function(a, d) {
  if (!both_named_lists(a, d)) return(d)
  out <- a
  for (f in names(d))
    out <- assign_keep_null(out, f,
      if (f == "value" && "value" %in% names(a))
        compose_registry_value(a$value, d$value) else d[[f]])
  out
}

# compose_registry_value(a, d): registry values are single-keyword
# replace) -- replaces the expression (REQ-0628).
compose_registry_value <- function(a, d) {
  ka <- reg_key(a)
  kd <- reg_key(d)
  if (!is.null(ka) && !is.null(kd) && ka == kd &&
      both_named_lists(a[[ka]], d[[kd]]))
    return(setNames(list(merge_named(a[[ka]], d[[kd]])), kd))
  d
}

reg_key <- function(x) {
  nm <- names(x)
  if (is.list(x) && !is.null(nm) && length(x) == 1 && nzchar(nm[1])) nm[1]
  else NULL
}

# ---- verification defaults ------------------------------------------------
# materialize_verif_defaults(acc): append severity: error to any
# verification payload that omits it (the schema default, REQ-0248).
materialize_verif_defaults <- function(acc) {
  if (!is.null(acc$verifications))
    acc$verifications <- lapply(acc$verifications, materialize_one_verif)
  acc$columns <- lapply(acc$columns, function(c) {
    if (!is.null(c$verifications))
      c$verifications <- lapply(c$verifications, materialize_one_verif)
    c
  })
  acc
}

materialize_one_verif <- function(v) {
  if (is.null(v)) return(v)
  k <- names(v)[1]
  p <- v[[1]]
  if (is.list(p) && !is.null(names(p)) && !"severity" %in% names(p))
    p$severity <- "error"
  v[[1]] <- p
  v
}

# ---- reachability pruning -------------------------------------------------
# inherit_refs(x, colnames): column and dataset/intermediate references in
inherit_refs <- function(x, colnames) {
  cols <- character(0)
  quals <- character(0)
  add_ast_ids <- function(node) {
    walk <- function(n) {
      if (!is.list(n)) return()
      if (identical(n$kind, "id")) {
        if (is.null(n$qualifier) && n$name %in% colnames)
          cols <<- c(cols, n$name)
        else if (!is.null(n$qualifier))
          quals <<- c(quals, n$qualifier)
      }
      for (e in n) walk(e)
    }
    walk(node)
  }
  add_str <- function(s) {
    q <- split_qual(s)
    if (!is.null(q$q)) quals <<- c(quals, q$q)
    else if (q$v %in% colnames) cols <<- c(cols, q$v)
  }
  walk_string <- function(s, key = NULL, parent = NULL) {
    if (!is.null(key) && key %in% c("when", "then", "filter")) {
      node <- tryCatch(parse_predicate_text(s), error = function(e) NULL)
      if (!is.null(node)) add_ast_ids(node)
      return()
    }
    if (identical(key, "expr") && identical(parent, "compute")) {
      node <- tryCatch(parse_compute_text(s), error = function(e) NULL)
      if (!is.null(node)) add_ast_ids(node)
      return()
    }
    if (identical(key, "aggregate") || identical(parent, "aggregate")) {
      node <- tryCatch(parse_aggregate_text(s), error = function(e) NULL)
      if (!is.null(node)) add_ast_ids(node)
      return()
    }
    if (!is.null(key) && key %in% c("source", "sources", "variable", "value",
        "date", "key_base", "not_before", "group_by", "order_by")) {
      add_str(s)
      return()
    }
    if (identical(parent, "str_template")) {
      for (m in regmatches(s,
          gregexpr("\\{[A-Za-z_][A-Za-z0-9_.]*\\}", s, perl = TRUE))[[1]])
        add_str(substr(m, 2, nchar(m) - 1))
      return()
    }
    if (is.null(key)) add_str(s)
  }
  walk <- function(x, key = NULL, parent = NULL) {
    if (is.character(x)) {
      for (s in x) walk_string(s, key, parent)
      return()
    }
    if (!is.list(x)) return()
    if (!is.null(names(x)) && "value" %in% names(x)) {
      walk(x$value, key, parent)
      return()
    }
    nm <- names(x)
    if (is.null(nm)) {
      for (e in x) walk(e, NULL, parent)
      return()
    }
    for (i in seq_along(x)) {
      k <- nm[i]
      pk <- if (k %in% EXPR_KINDS) k else parent
      walk(x[[i]], if (nzchar(k)) k else NULL, pk)
    }
  }
  walk(x, NULL, NULL)
  list(cols = unique(cols), quals = unique(quals))
}

# unqual_vars(xs): unqualified variable names in a variable list.
unqual_vars <- function(xs) {
  out <- character(0)
  for (s in xs) {
    if (!is.character(s) || length(s) != 1) next
    q <- split_qual(s)
    if (is.null(q$q)) out <- c(out, q$v)
  }
  unique(out)
}

# verif_col_refs(v, colnames): columns a verification touches. The
verif_col_refs <- function(v, colnames) {
  if (is.null(v)) return(character(0))
  out <- character(0)
  p <- v[[1]]
  if (is.list(p) && !is.null(names(p))) {
    if (!is.null(p$columns)) out <- c(out, unqual_vars(p$columns))
    for (f in intersect(c("when", "condition", "expression"), names(p))) {
      s <- p[[f]]
      if (is.character(s) && length(s) == 1) {
        node <- tryCatch(parse_predicate_text(s), error = function(e) NULL)
        if (!is.null(node))
          out <- c(out, ast_col_refs(node, colnames))
      }
    }
  }
  unique(out)
}

# prune_composed(spec): reachability analysis over the composed object.
# Roots: every final row declaration (a row can add records, REQ-0640),
# inputs, intermediates and columns only they referenced (REQ-0639).
# verification cannot report on a pruned column, REQ-0642).
prune_composed <- function(spec) {
  colnames <- vapply(spec$columns, function(c) c$name, character(1))
  ds_ids <- names(spec$input)
  if (is.null(ds_ids)) ds_ids <- character(0)
  im_ids <- if (is.null(spec$intermediates)) character(0) else
    vapply(spec$intermediates, function(m) m$id, character(1))
  live_c <- union(spec$output$columns, spec$keys)
  if (!is.null(spec$output$order_by))
    live_c <- union(live_c, unqual_vars(spec$output$order_by))
  for (v in spec$verifications) live_c <- union(live_c, verif_col_refs(v, colnames))
  for (c in spec$columns)
    if (!is.null(c$verifications)) live_c <- union(live_c, c$name)
  live_ds <- spec$base
  live_im <- character(0)
  changed <- TRUE
  while (changed) {
    changed <- FALSE
    add_c <- function(x) {
      nx <- setdiff(intersect(x, colnames), live_c)
      if (length(nx)) {
        live_c <<- union(live_c, nx)
        changed <<- TRUE
      }
    }
    add_ds <- function(x) {
      nx <- setdiff(intersect(x, ds_ids), live_ds)
      if (length(nx)) {
        live_ds <<- union(live_ds, nx)
        changed <<- TRUE
      }
    }
    add_im <- function(x) {
      nx <- setdiff(intersect(x, im_ids), live_im)
      if (length(nx)) {
        live_im <<- union(live_im, nx)
        changed <<- TRUE
      }
    }
    feed <- function(r) {
      add_c(r$cols)
      add_ds(r$quals)
      add_im(r$quals)
    }
    for (c in spec$columns) {
      if (!c$name %in% live_c) next
      feed(inherit_refs(c$derivation, colnames))
    }
    for (t in spec$rows) {  # rows are roots: every row declaration feeds
      if (!is.null(t$derivations))
        for (d in t$derivations) feed(inherit_refs(d, colnames))
      feed(inherit_refs(list(filter = t$filter, group_by = t$group_by),
        colnames))
    }
    for (m in spec$intermediates) {
      if (!m$id %in% live_im) next
      add_ds(m$dataset)
      if (!is.null(m$key_base))
        add_c(unqual_vars(m$key_base))
      feed(inherit_refs(list(filter = m$filter), colnames))
    }
    for (v in spec$verifications)
      feed(list(cols = verif_col_refs(v, colnames), quals = character(0)))
  }
  spec$columns <- Filter(function(c) c$name %in% live_c, spec$columns)
  if (!is.null(spec$input)) {
    spec$input <- spec$input[names(spec$input) %in% live_ds]
    if (length(spec$input) == 0) spec$input <- NULL
  }
  if (!is.null(spec$intermediates)) {
    spec$intermediates <- Filter(function(m) m$id %in% live_im,
      spec$intermediates)
    if (length(spec$intermediates) == 0) spec$intermediates <- NULL
  }
  if (!is.null(spec$rows))
    spec$rows <- lapply(spec$rows, function(t) {
      if (!is.null(t$derivations)) {
        t$derivations <- t$derivations[names(t$derivations) %in% live_c]
        if (length(t$derivations) == 0) t$derivations <- NULL
      }
      t
    })
  spec
}

# ---- deterministic column order ------------------------------------------
# order_composed_columns(spec): the final column list follows the
# by first-contribution order (REQ-0643). Reuses the engine's own ordering.
order_composed_columns <- function(spec) {
  colnames <- vapply(spec$columns, function(c) c$name, character(1))
  deps <- lapply(spec$columns, function(c) deriv_refs(c$derivation, colnames))
  names(deps) <- colnames
  if (has_cycle(colnames, deps))
    yamaa_error("dependency_cycle",
      "column derivations contain a dependency cycle")
  keys <- spec$keys
  if (is.null(keys)) keys <- character(0)
  ideps <- lapply(spec$columns, function(c)
    if (deriv_needs_keys(c$derivation)) intersect(keys, colnames)
    else character(0))
  names(ideps) <- colnames
  pos <- setNames(seq_along(colnames), colnames)  # first-contribution order
  ord <- topo_order(colnames, deps, ideps, pos)
  spec$columns <- spec$columns[match(ord, colnames)]
  spec
}

# ---- final materialization ------------------------------------------------
INHERIT_ROOT_ORDER <- c("schema_version", "domain", "keys", "input", "base",
  "intermediates", "output", "columns", "rows", "filter", "verifications",
  "submission", "metadata")
INHERIT_INPUT_ORDER <- c("path", "types", "schema", "empty_string")
INHERIT_INTERMEDIATE_ORDER <- c("id", "dataset", "key", "key_base", "between",
  "filter", "order_by", "keep", "columns", "missing", "strict")
INHERIT_COLUMN_ORDER <- c("name", "type", "label", "derivation",
  "verifications", "submission", "metadata")
INHERIT_ROW_ORDER <- c("id", "dataset", "group_by", "filter", "derivations",
  "submission")

reorder_fields <- function(x, order) {
  if (is.null(x) || is.null(names(x))) return(x)
  perm <- order(match(names(x), order), seq_along(x), na.last = TRUE)
  x[perm]
}

# finalize_composed(spec): materialize the resolved object in schema
# field order (REQ-0649). The parents field is gone; the entry contributes
finalize_composed <- function(spec) {
  spec <- reorder_fields(spec, INHERIT_ROOT_ORDER)
  if (!is.null(spec$input))
    spec$input <- lapply(spec$input, function(m)
      reorder_fields(m, INHERIT_INPUT_ORDER))
  if (!is.null(spec$intermediates))
    spec$intermediates <- lapply(spec$intermediates, function(m)
      reorder_fields(m, INHERIT_INTERMEDIATE_ORDER))
  if (!is.null(spec$columns))
    spec$columns <- lapply(spec$columns, function(c)
      reorder_fields(c, INHERIT_COLUMN_ORDER))
  if (!is.null(spec$rows))
    spec$rows <- lapply(spec$rows, function(t)
      reorder_fields(t, INHERIT_ROW_ORDER))
  spec
}
