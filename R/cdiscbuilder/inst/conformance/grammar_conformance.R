# Replay the shared grammar vectors against the R parsers.
#
# `yaml/grammar/` holds one machine-readable grammar per closed language.
# Repository validation reads those files against the rule that owns each
# grammar and against the Python parser; this script reads the same files
# against the R parser, so the two runtimes are tested from one source
# instead of two transcriptions.
#
# From the repository root:
#
#   Rscript R/cdiscbuilder/inst/conformance/grammar_conformance.R [root]
#
# It prints one line per disagreement and exits non-zero when there is any.

YAMAA_GRAMMAR_CONTRACTS <- c(
  predicate = "R004",
  numeric = "R010",
  `string-template` = "R012",
  aggregate = "R013"
)

#' The directory holding this script, however it was invoked
#' @noRd
.conformance_script_dir <- function() {
  arguments <- commandArgs(trailingOnly = FALSE)
  file_argument <- grep("^--file=", arguments, value = TRUE)
  if (length(file_argument) == 1L) {
    return(dirname(normalizePath(sub("^--file=", "", file_argument))))
  }
  getwd()
}

#' The production a grammar document defines under a name
#' @noRd
.grammar_production <- function(document, name) {
  for (production in document$productions) {
    if (identical(production$name, name)) {
      return(production)
    }
  }
  NULL
}

#' The terminals an EBNF definition quotes, in order
#' @noRd
.grammar_terminals <- function(definition) {
  quoted <- regmatches(definition, gregexpr('"[^"]*"', definition))[[1]]
  substr(quoted, 2L, nchar(quoted) - 1L)
}

#' Report a closed vocabulary the R parser reads differently
#' @noRd
.grammar_vocabulary_failures <- function(contract, document, label) {
  failures <- character(0)
  if (contract == "predicate") {
    reserved <- as.character(document$reserved)
    if (!setequal(reserved, YAMAA_PREDICATE_RESERVED)) {
      failures <- c(failures, paste0(
        label, ": reserved names differ from the ones this parser reserves"
      ))
    }
    compare <- .grammar_production(document, "compare")
    declared <- if (is.null(compare)) {
      character(0)
    } else {
      .grammar_terminals(compare$definition)
    }
    if (!identical(declared, YAMAA_PREDICATE_COMPARISON_OPERATORS)) {
      failures <- c(failures, paste0(
        label, ": compare declares operators this parser does not tokenize"
      ))
    }
    return(failures)
  }
  if (contract == "numeric") {
    functions <- document$vocabulary[["function"]]
    if (!setequal(names(functions), names(YAMAA_NUMERIC_FUNCTIONS))) {
      failures <- c(failures, paste0(
        label, ": vocabulary.function differs from this parser's functions"
      ))
    }
    for (name in intersect(names(functions), names(YAMAA_NUMERIC_FUNCTIONS))) {
      maximum <- functions[[name]]$max_arguments
      declared <- c(
        functions[[name]]$min_arguments,
        if (is.null(maximum)) NA else maximum
      )
      if (!identical(as.numeric(declared),
                     as.numeric(YAMAA_NUMERIC_FUNCTIONS[[name]]))) {
        failures <- c(failures, paste0(
          label, ": function ", name, " declares an arity this parser does ",
          "not enforce"
        ))
      }
    }
    prohibited <- unlist(document$prohibited)
    if (!identical(
      prohibited[order(names(prohibited))],
      YAMAA_PROHIBITED_KEYWORDS[order(names(YAMAA_PROHIBITED_KEYWORDS))]
    )) {
      failures <- c(failures, paste0(
        label, ": prohibited differs from the constructs this parser refuses"
      ))
    }
    return(failures)
  }
  if (contract == "aggregate") {
    reducers <- names(document$vocabulary$reducer)
    if (!setequal(reducers, YAMAA_AGGREGATE_REDUCERS)) {
      failures <- c(failures, paste0(
        label, ": vocabulary.reducer differs from this parser's reducers"
      ))
    }
    return(failures)
  }
  failures
}

#' Report every vector the R parser reads differently
#' @noRd
.grammar_case_failures <- function(contract, case, label) {
  failures <- character(0)
  decision <- yamaa_grammar_decision(contract, case$text)
  if (identical(case$parse, "reject")) {
    if (is.na(decision$condition)) {
      return(paste0(
        label, ": this parser accepts a text the vector records as rejected"
      ))
    }
    if (!identical(decision$condition, case$condition)) {
      return(paste0(
        label, ": this parser fails with '", decision$condition,
        "', not the '", case$condition, "' the vector records"
      ))
    }
    return(failures)
  }
  if (!identical(case$parse, "accept")) {
    return(paste0(label, ": parse must be 'accept' or 'reject'"))
  }
  if (!is.na(decision$condition)) {
    return(paste0(
      label, ": this parser rejects an accepted text with '",
      decision$condition, "'"
    ))
  }
  identifiers <- as.character(unlist(case$identifiers))
  if (!identical(decision$identifiers, identifiers)) {
    failures <- c(failures, paste0(
      label, ": this parser collects [", paste(decision$identifiers,
        collapse = ", "
      ), "], not the [", paste(identifiers, collapse = ", "),
      "] the vector records"
    ))
  }
  if (!identical(decision$shape, case$shape)) {
    failures <- c(failures, paste0(
      label, ": this parser produces ", decision$shape, ", not the ",
      case$shape, " the vector records"
    ))
  }
  failures
}

#' Replay every shared grammar vector against the R parsers
#'
#' @param root Repository root holding `yaml/grammar/`.
#' @return A character vector of disagreements, empty when there are none.
#' @noRd
yamaa_grammar_conformance <- function(root = ".") {
  failures <- character(0)
  for (contract in sort(names(YAMAA_GRAMMAR_CONTRACTS))) {
    label <- file.path("yaml", "grammar", paste0(contract, ".yaml"))
    path <- file.path(root, label)
    if (!file.exists(path)) {
      failures <- c(failures, paste0(label, ": missing grammar"))
      next
    }
    document <- yaml::read_yaml(path)
    if (!identical(document$rule, unname(YAMAA_GRAMMAR_CONTRACTS[contract]))) {
      failures <- c(failures, paste0(
        label, ": rule must be ", YAMAA_GRAMMAR_CONTRACTS[contract]
      ))
    }
    failures <- c(
      failures, .grammar_vocabulary_failures(contract, document, label)
    )
    for (case in document$cases) {
      failures <- c(failures, .grammar_case_failures(
        contract, case, paste0(label, ": ", case$id)
      ))
    }
  }
  failures
}

if (sys.nframe() == 0L) {
  script_dir <- .conformance_script_dir()
  arguments <- commandArgs(trailingOnly = TRUE)
  repository_root <- if (length(arguments) >= 1L) {
    arguments[1]
  } else {
    normalizePath(file.path(script_dir, "..", "..", "..", ".."))
  }
  source(file.path(script_dir, "..", "..", "R", "grammar.R"))
  reported <- yamaa_grammar_conformance(repository_root)
  if (length(reported) > 0L) {
    cat(paste0("ERROR: ", reported, collapse = "\n"), "\n", sep = "")
    quit(status = 1L)
  }
  cat("PASS: the R parsers reproduce every shared grammar vector.\n")
}
