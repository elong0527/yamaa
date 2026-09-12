# Parsers for the four closed grammars the language defines.
#
# `yaml/grammar/` holds one machine-readable grammar per language and the
# vectors every implementation must reproduce. This file is the R reading of
# those grammars; `inst/conformance/grammar_conformance.R` replays the shared
# vectors against it, so a transcription that drifts from the grammar files
# fails rather than accepting a specification the Python implementation
# rejects.
#
# Only base R is used, so the conformance runner can source this file without
# loading the package. The parsers decide what the grammars admit; evaluating
# an accepted expression is not theirs, and the package's SDTM and ADaM
# engines do not yet read specifications through them.

#' R004's closed vocabulary
#' @noRd
YAMAA_PREDICATE_RESERVED <- c(
  "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS",
  "LIKE", "NOT", "NULL", "OR", "TRUE"
)

#' R004's comparison operators, longest spelling first while tokenizing
#' @noRd
YAMAA_PREDICATE_COMPARISON_OPERATORS <- c("=", "<>", "<", "<=", ">", ">=")

#' R010's closed function vocabulary, as minimum and maximum arity
#' @noRd
YAMAA_NUMERIC_FUNCTIONS <- list(
  ABS = c(1, 1),
  CEIL = c(1, 1),
  FLOOR = c(1, 1),
  TRUNC = c(1, 1),
  SQRT = c(1, 1),
  POWER = c(2, 2),
  EXP = c(1, 1),
  LN = c(1, 1),
  MOD = c(2, 2),
  GREATEST = c(2, NA),
  LEAST = c(2, NA),
  NULLIF = c(2, 2),
  COALESCE = c(1, NA)
)

#' Spellings R010 and R013 reserve for constructs they do not admit
#' @noRd
YAMAA_PROHIBITED_KEYWORDS <- c(
  AND = "boolean",
  BETWEEN = "comparison",
  CASE = "conditional",
  ELSE = "conditional",
  END = "conditional",
  `FALSE` = "boolean",
  IN = "comparison",
  IS = "comparison",
  LIKE = "comparison",
  NOT = "boolean",
  OR = "boolean",
  OVER = "window",
  THEN = "conditional",
  `TRUE` = "boolean",
  WHEN = "conditional"
)

#' R013's closed reducer vocabulary
#' @noRd
YAMAA_AGGREGATE_REDUCERS <- c("SUM", "COUNT", "MIN", "MAX", "MEAN", "ONLY")

#' Fail with the condition the owning rule registers
#' @noRd
.grammar_stop <- function(message, condition_name) {
  stop(structure(
    class = c("yamaa_grammar_error", "error", "condition"),
    list(message = message, call = NULL, condition_name = condition_name)
  ))
}

#' Match a regular expression at the start of the remaining text
#' @noRd
.grammar_match <- function(text, index, pattern) {
  remainder <- substr(text, index, nchar(text))
  matched <- regmatches(remainder, regexpr(pattern, remainder))
  if (length(matched) == 0L || !nzchar(matched)) NULL else matched
}

#' Whether a year, month, and day name a day that exists
#' @noRd
.grammar_valid_date <- function(year, month, day) {
  if (month < 1L || month > 12L || day < 1L) {
    return(FALSE)
  }
  lengths <- c(31L, 28L, 31L, 30L, 31L, 30L, 31L, 31L, 30L, 31L, 30L, 31L)
  leap <- (year %% 4L == 0L && year %% 100L != 0L) || year %% 400L == 0L
  if (month == 2L && leap) {
    return(day <= 29L)
  }
  day <= lengths[month]
}

#' Whether a temporal literal parses under R016 for its named type
#' @noRd
.grammar_valid_temporal <- function(kind, value) {
  if (kind == "date") {
    if (!grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}$", value)) {
      return(FALSE)
    }
    parts <- as.integer(strsplit(value, "-", fixed = TRUE)[[1]])
    return(.grammar_valid_date(parts[1], parts[2], parts[3]))
  }
  pattern <- paste0(
    "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}", "(:[0-9]{2})?$"
  )
  if (!grepl(pattern, value)) {
    return(FALSE)
  }
  halves <- strsplit(value, "T", fixed = TRUE)[[1]]
  date_parts <- as.integer(strsplit(halves[1], "-", fixed = TRUE)[[1]])
  if (!.grammar_valid_date(date_parts[1], date_parts[2], date_parts[3])) {
    return(FALSE)
  }
  time_parts <- as.integer(strsplit(halves[2], ":", fixed = TRUE)[[1]])
  if (length(time_parts) == 2L) {
    time_parts <- c(time_parts, 0L)
  }
  time_parts[1] <= 23L && time_parts[2] <= 59L && time_parts[3] <= 59L
}

#' Whether a LIKE pattern ends in an escape character with nothing to escape
#' @noRd
.grammar_dangling_escape <- function(pattern, escape) {
  escaped <- FALSE
  for (character in strsplit(pattern, "")[[1]]) {
    if (escaped) {
      escaped <- FALSE
    } else if (character == escape) {
      escaped <- TRUE
    }
  }
  escaped
}

#' Tokenize the closed R004 predicate language
#' @noRd
.tokenize_predicate <- function(text) {
  operators <- YAMAA_PREDICATE_COMPARISON_OPERATORS
  two_character <- operators[nchar(operators) == 2L]
  one_character <- operators[nchar(operators) == 1L]
  tokens <- list()
  index <- 1L
  end <- nchar(text)
  while (index <= end) {
    character <- substr(text, index, index)
    if (grepl("^[[:space:]]$", character)) {
      index <- index + 1L
      next
    }
    if (character == "'") {
      start <- index
      index <- index + 1L
      value <- character(0)
      closed <- FALSE
      while (index <= end) {
        current <- substr(text, index, index)
        if (current != "'") {
          value <- c(value, current)
          index <- index + 1L
          next
        }
        if (index < end && substr(text, index + 1L, index + 1L) == "'") {
          value <- c(value, "'")
          index <- index + 2L
          next
        }
        index <- index + 1L
        tokens[[length(tokens) + 1L]] <- list(
          kind = "STRING", value = paste(value, collapse = ""),
          position = start
        )
        closed <- TRUE
        break
      }
      if (!closed) {
        .grammar_stop("unterminated string literal", "invalid_predicate")
      }
      next
    }
    number <- .grammar_match(
      text, index, "^[+-]?[0-9]+(\\.[0-9]+)?([eE][+-]?[0-9]+)?"
    )
    if (!is.null(number)) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = "NUMBER", value = number, position = index
      )
      index <- index + nchar(number)
      next
    }
    name <- .grammar_match(text, index, "^[A-Za-z_][A-Za-z0-9_]*")
    if (!is.null(name)) {
      stop_index <- index + nchar(name)
      if (stop_index <= end &&
        substr(text, stop_index, stop_index) == ".") {
        suffix <- .grammar_match(
          text, stop_index + 1L, "^[A-Za-z_][A-Za-z0-9_]*"
        )
        if (is.null(suffix)) {
          .grammar_stop("invalid qualified identifier", "invalid_predicate")
        }
        name <- paste0(name, ".", suffix)
        stop_index <- stop_index + 1L + nchar(suffix)
      }
      tokens[[length(tokens) + 1L]] <- list(
        kind = "NAME", value = name, position = index
      )
      index <- stop_index
      next
    }
    pair <- substr(text, index, min(index + 1L, end))
    if (nchar(pair) == 2L && pair %in% two_character) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = "OP", value = pair, position = index
      )
      index <- index + 2L
      next
    }
    if (character %in% one_character) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = "OP", value = character, position = index
      )
      index <- index + 1L
      next
    }
    punctuation <- c("(" = "LPAREN", ")" = "RPAREN", "," = "COMMA")
    if (character %in% names(punctuation)) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = unname(punctuation[character]), value = character,
        position = index
      )
      index <- index + 1L
      next
    }
    .grammar_stop(
      paste0("unexpected character '", character, "'"), "invalid_predicate"
    )
  }
  tokens[[length(tokens) + 1L]] <- list(
    kind = "EOF", value = "", position = end + 1L
  )
  tokens
}

#' @noRd
.parser_state <- function(tokens) {
  state <- new.env(parent = emptyenv())
  state$tokens <- tokens
  state$index <- 1L
  state
}

#' @noRd
.peek <- function(state) state$tokens[[state$index]]

#' @noRd
.advance <- function(state) {
  token <- state$tokens[[state$index]]
  state$index <- state$index + 1L
  token
}

#' @noRd
.is_keyword <- function(state, value) {
  token <- .peek(state)
  token$kind == "NAME" && toupper(token$value) == value
}

#' @noRd
.take_keyword <- function(state, value) {
  if (.is_keyword(state, value)) .advance(state) else NULL
}

#' @noRd
.require_kind <- function(state, kind, message, condition_name) {
  if (.peek(state)$kind != kind) {
    .grammar_stop(message, condition_name)
  }
  .advance(state)
}

#' @noRd
.parse_predicate_operand <- function(state) {
  token <- .peek(state)
  if (token$kind == "NUMBER") {
    .advance(state)
    value_type <- if (grepl("[.eE]", token$value)) "float" else "int"
    return(list(kind = "literal", type = value_type, value = token$value))
  }
  if (token$kind == "STRING") {
    .advance(state)
    return(list(kind = "literal", type = "str", value = token$value))
  }
  if (!is.null(.take_keyword(state, "NULL"))) {
    return(list(kind = "literal", type = NA_character_, value = NA_character_))
  }
  if (.is_keyword(state, "DATE") || .is_keyword(state, "DATETIME")) {
    value_type <- tolower(.advance(state)$value)
    literal <- .require_kind(
      state, "STRING",
      paste0(toupper(value_type), " requires a string literal"),
      "invalid_predicate"
    )
    if (!.grammar_valid_temporal(value_type, literal$value)) {
      .grammar_stop(
        paste0("invalid ", value_type, " literal"), "invalid_predicate"
      )
    }
    return(list(kind = "literal", type = value_type, value = literal$value))
  }
  if (token$kind == "NAME") {
    if (toupper(token$value) %in% YAMAA_PREDICATE_RESERVED) {
      .grammar_stop("expected operand", "invalid_predicate")
    }
    .advance(state)
    return(list(kind = "identifier", name = token$value))
  }
  .grammar_stop("expected operand", "invalid_predicate")
}

#' @noRd
.parse_predicate_boolean <- function(state) {
  if (.peek(state)$kind == "LPAREN") {
    .advance(state)
    node <- .parse_predicate_disjunction(state)
    .require_kind(
      state, "RPAREN", "expected ')' to close predicate", "invalid_predicate"
    )
    return(node)
  }
  if (!is.null(.take_keyword(state, "TRUE"))) {
    return(list(kind = "boolean", value = TRUE))
  }
  if (!is.null(.take_keyword(state, "FALSE"))) {
    return(list(kind = "boolean", value = FALSE))
  }

  left <- .parse_predicate_operand(state)
  if (.peek(state)$kind == "OP") {
    operator <- .advance(state)$value
    return(list(
      kind = "comparison", operator = operator, left = left,
      right = .parse_predicate_operand(state)
    ))
  }
  if (!is.null(.take_keyword(state, "IS"))) {
    negated <- !is.null(.take_keyword(state, "NOT"))
    if (is.null(.take_keyword(state, "NULL"))) {
      .grammar_stop("expected NULL after IS", "invalid_predicate")
    }
    return(list(kind = "null_test", value = left, negated = negated))
  }

  negated <- !is.null(.take_keyword(state, "NOT"))
  if (!is.null(.take_keyword(state, "IN"))) {
    .require_kind(
      state, "LPAREN", "expected '(' after IN", "invalid_predicate"
    )
    values <- list(.parse_predicate_operand(state))
    while (.peek(state)$kind == "COMMA") {
      .advance(state)
      values[[length(values) + 1L]] <- .parse_predicate_operand(state)
    }
    .require_kind(
      state, "RPAREN", "expected ')' after IN operands", "invalid_predicate"
    )
    return(list(kind = "in", value = left, values = values, negated = negated))
  }
  if (!is.null(.take_keyword(state, "BETWEEN"))) {
    lower <- .parse_predicate_operand(state)
    if (is.null(.take_keyword(state, "AND"))) {
      .grammar_stop("expected AND in BETWEEN predicate", "invalid_predicate")
    }
    return(list(
      kind = "between", value = left, lower = lower,
      upper = .parse_predicate_operand(state), negated = negated
    ))
  }
  if (!is.null(.take_keyword(state, "LIKE"))) {
    pattern <- .parse_predicate_operand(state)
    escape <- NULL
    if (!is.null(.take_keyword(state, "ESCAPE"))) {
      literal <- .require_kind(
        state, "STRING", "ESCAPE requires a string literal",
        "invalid_predicate"
      )
      if (nchar(literal$value) != 1L) {
        .grammar_stop(
          "ESCAPE requires exactly one code point", "invalid_predicate"
        )
      }
      escape <- literal$value
    }
    if (!is.null(escape) && pattern$kind == "literal" &&
      identical(pattern$type, "str") &&
      .grammar_dangling_escape(pattern$value, escape)) {
      .grammar_stop("LIKE pattern has a dangling escape", "invalid_predicate")
    }
    return(list(
      kind = "like", value = left, pattern = pattern, escape = escape,
      negated = negated
    ))
  }
  if (negated) {
    .grammar_stop(
      "NOT must precede IN, BETWEEN, or LIKE", "invalid_predicate"
    )
  }
  .grammar_stop(
    "operand must be followed by a Boolean operator", "invalid_predicate"
  )
}

#' @noRd
.parse_predicate_negation <- function(state) {
  if (!is.null(.take_keyword(state, "NOT"))) {
    return(list(kind = "not", value = .parse_predicate_negation(state)))
  }
  .parse_predicate_boolean(state)
}

#' @noRd
.parse_predicate_conjunction <- function(state) {
  node <- .parse_predicate_negation(state)
  while (!is.null(.take_keyword(state, "AND"))) {
    node <- list(
      kind = "and", left = node, right = .parse_predicate_negation(state)
    )
  }
  node
}

#' @noRd
.parse_predicate_disjunction <- function(state) {
  node <- .parse_predicate_conjunction(state)
  while (!is.null(.take_keyword(state, "OR"))) {
    node <- list(
      kind = "or", left = node, right = .parse_predicate_conjunction(state)
    )
  }
  node
}

#' Parse one R004 predicate
#' @noRd
yamaa_parse_predicate <- function(text) {
  if (!is.character(text) || length(text) != 1L || !nzchar(text)) {
    .grammar_stop(
      "predicate must be a non-empty string", "invalid_predicate"
    )
  }
  state <- .parser_state(.tokenize_predicate(text))
  node <- .parse_predicate_disjunction(state)
  if (.peek(state)$kind != "EOF") {
    .grammar_stop("unexpected trailing token", "invalid_predicate")
  }
  node
}

#' Tokenize the closed R010 and R013 expression languages
#'
#' The two share every token but the record star, which only R013 admits, and
#' the condition their failures carry.
#' @noRd
.tokenize_expression <- function(text, allow_star, condition_name) {
  tokens <- list()
  index <- 1L
  end <- nchar(text)
  punctuation <- c(
    "+" = "PLUS", "-" = "MINUS", "*" = "STAR", "/" = "SLASH",
    "(" = "LPAREN", ")" = "RPAREN", "," = "COMMA"
  )
  while (index <= end) {
    character <- substr(text, index, index)
    if (grepl("^[[:space:]]$", character)) {
      index <- index + 1L
      next
    }
    number <- .grammar_match(
      text, index, "^[0-9]+(\\.[0-9]+)?([eE][+-]?[0-9]+)?"
    )
    if (!is.null(number)) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = "NUMBER", value = number, position = index
      )
      index <- index + nchar(number)
      next
    }
    name <- .grammar_match(text, index, "^[A-Za-z_][A-Za-z0-9_]*")
    if (!is.null(name)) {
      stop_index <- index + nchar(name)
      kind <- "NAME"
      if (stop_index <= end &&
        substr(text, stop_index, stop_index) == ".") {
        after <- substr(text, stop_index + 1L, stop_index + 1L)
        if (allow_star && stop_index < end && after == "*") {
          name <- paste0(name, ".*")
          stop_index <- stop_index + 2L
          kind <- "QUALIFIED_STAR"
        } else {
          suffix <- .grammar_match(
            text, stop_index + 1L, "^[A-Za-z_][A-Za-z0-9_]*"
          )
          if (is.null(suffix)) {
            .grammar_stop("invalid qualified identifier", condition_name)
          }
          name <- paste0(name, ".", suffix)
          stop_index <- stop_index + 1L + nchar(suffix)
        }
      }
      construct <- YAMAA_PROHIBITED_KEYWORDS[toupper(name)]
      if (!grepl(".", name, fixed = TRUE) && !is.na(construct)) {
        .grammar_stop(
          paste0(construct, " construct is not permitted"),
          "prohibited_construct"
        )
      }
      tokens[[length(tokens) + 1L]] <- list(
        kind = kind, value = name, position = index
      )
      index <- stop_index
      next
    }
    if (character %in% names(punctuation)) {
      tokens[[length(tokens) + 1L]] <- list(
        kind = unname(punctuation[character]), value = character,
        position = index
      )
      index <- index + 1L
      next
    }
    if (character %in% c("<", ">", "=", "!")) {
      .grammar_stop("comparison is not permitted", "prohibited_construct")
    }
    if (character == "'") {
      .grammar_stop(
        "string literals are not permitted", "prohibited_construct"
      )
    }
    .grammar_stop(
      paste0("unexpected character '", character, "'"), condition_name
    )
  }
  tokens[[length(tokens) + 1L]] <- list(
    kind = "EOF", value = "", position = end + 1L
  )
  tokens
}

#' @noRd
.parse_expression_primary <- function(state, options) {
  token <- .peek(state)
  if (token$kind == "NUMBER") {
    .advance(state)
    value_type <- if (grepl("[.eE]", token$value)) "float" else "int"
    return(list(kind = "number", type = value_type, value = token$value))
  }
  if (token$kind == "NAME") {
    .advance(state)
    keyword <- toupper(token$value)
    if (.peek(state)$kind != "LPAREN") {
      if (keyword == "NULL") {
        return(list(kind = "null"))
      }
      return(list(kind = "identifier", name = token$value))
    }
    .advance(state)
    if (options$reducers && keyword %in% YAMAA_AGGREGATE_REDUCERS) {
      if (.peek(state)$kind == "QUALIFIED_STAR") {
        star <- .advance(state)
        argument <- list(
          kind = "qualified_star",
          dataset = substr(star$value, 1L, nchar(star$value) - 2L)
        )
      } else {
        argument <- .parse_expression(state, options)
      }
      .require_kind(
        state, "RPAREN", "expected ')' to close reducer", options$condition
      )
      return(list(
        kind = "reduction", name = token$value, argument = argument
      ))
    }
    arguments <- list()
    if (.peek(state)$kind != "RPAREN") {
      arguments[[1L]] <- .parse_expression(state, options)
      while (.peek(state)$kind == "COMMA") {
        .advance(state)
        arguments[[length(arguments) + 1L]] <- .parse_expression(
          state, options
        )
      }
    }
    .require_kind(
      state, "RPAREN", "expected ')' to close function", options$condition
    )
    return(list(kind = "call", name = token$value, arguments = arguments))
  }
  if (token$kind == "LPAREN") {
    .advance(state)
    node <- .parse_expression(state, options)
    .require_kind(
      state, "RPAREN", "expected ')' to close expression", options$condition
    )
    return(node)
  }
  if (token$kind == "QUALIFIED_STAR") {
    .grammar_stop(
      "qualified star is valid only as COUNT argument", options$condition
    )
  }
  .grammar_stop("expected a primary expression", options$condition)
}

#' @noRd
.parse_expression_factor <- function(state, options) {
  if (.peek(state)$kind %in% c("PLUS", "MINUS")) {
    operator <- .advance(state)$value
    return(list(
      kind = "unary", operator = operator,
      value = .parse_expression_primary(state, options)
    ))
  }
  .parse_expression_primary(state, options)
}

#' @noRd
.parse_expression_term <- function(state, options) {
  node <- .parse_expression_factor(state, options)
  while (.peek(state)$kind %in% c("STAR", "SLASH")) {
    operator <- .advance(state)$value
    node <- list(
      kind = "binary", operator = operator, left = node,
      right = .parse_expression_factor(state, options)
    )
  }
  node
}

#' @noRd
.parse_expression <- function(state, options) {
  node <- .parse_expression_term(state, options)
  while (.peek(state)$kind %in% c("PLUS", "MINUS")) {
    operator <- .advance(state)$value
    node <- list(
      kind = "binary", operator = operator, left = node,
      right = .parse_expression_term(state, options)
    )
  }
  node
}

#' @noRd
.parse_closed_expression <- function(text, reducers, condition_name) {
  if (!is.character(text) || length(text) != 1L || !nzchar(text)) {
    .grammar_stop("expression must be a non-empty string", condition_name)
  }
  options <- list(reducers = reducers, condition = condition_name)
  state <- .parser_state(
    .tokenize_expression(text, reducers, condition_name)
  )
  node <- .parse_expression(state, options)
  if (.peek(state)$kind != "EOF") {
    .grammar_stop("unexpected trailing token", condition_name)
  }
  node
}

#' Parse one R010 scalar numeric expression
#' @noRd
yamaa_parse_numeric_expression <- function(text) {
  .parse_closed_expression(text, FALSE, "invalid_numeric_expression")
}

#' Parse one R013 aggregate expression
#' @noRd
yamaa_parse_aggregate_expression <- function(text) {
  .parse_closed_expression(text, TRUE, "invalid_aggregate_expression")
}

#' Conditions R010's closed vocabulary reports for a parsed expression
#'
#' Arguments are read before the name they are passed to, so the reported
#' condition is the first one a left-to-right reading reaches.
#' @noRd
.numeric_conditions <- function(node) {
  kind <- node$kind
  if (kind == "unary") {
    return(.numeric_conditions(node$value))
  }
  if (kind == "binary") {
    return(c(
      .numeric_conditions(node$left), .numeric_conditions(node$right)
    ))
  }
  if (kind != "call") {
    return(character(0))
  }
  conditions <- character(0)
  for (argument in node$arguments) {
    conditions <- c(conditions, .numeric_conditions(argument))
  }
  arity <- YAMAA_NUMERIC_FUNCTIONS[[toupper(node$name)]]
  if (is.null(arity)) {
    return(c(conditions, "prohibited_function"))
  }
  count <- length(node$arguments)
  if (count < arity[1] || (!is.na(arity[2]) && count > arity[2])) {
    return(c(conditions, "prohibited_function"))
  }
  conditions
}

#' Conditions R013's closed vocabulary reports for a parsed expression
#' @noRd
.aggregate_conditions <- function(node, enclosing = NULL) {
  kind <- node$kind
  if (kind == "unary") {
    return(.aggregate_conditions(node$value, enclosing))
  }
  if (kind == "binary") {
    return(c(
      .aggregate_conditions(node$left, enclosing),
      .aggregate_conditions(node$right, enclosing)
    ))
  }
  if (kind == "call") {
    conditions <- character(0)
    for (argument in node$arguments) {
      conditions <- c(conditions, .aggregate_conditions(argument, enclosing))
    }
    arity <- YAMAA_NUMERIC_FUNCTIONS[[toupper(node$name)]]
    if (is.null(arity)) {
      return(c(conditions, "prohibited_function"))
    }
    count <- length(node$arguments)
    if (count < arity[1] || (!is.na(arity[2]) && count > arity[2])) {
      return(c(conditions, "prohibited_function"))
    }
    return(conditions)
  }
  if (kind != "reduction") {
    return(character(0))
  }
  conditions <- character(0)
  if (!is.null(enclosing)) {
    conditions <- c(conditions, "nested_reduction")
  }
  if (node$argument$kind == "qualified_star") {
    if (toupper(node$name) != "COUNT") {
      return(c(conditions, "invalid_aggregate_expression"))
    }
    return(conditions)
  }
  c(conditions, .aggregate_conditions(node$argument, node$name))
}

#' Parse one R012 template into its literal text and placeholder parts
#' @noRd
yamaa_parse_string_template <- function(text) {
  if (!is.character(text) || length(text) != 1L) {
    .grammar_stop(
      "string template must be a string", "invalid_string_template"
    )
  }
  parts <- list()
  literal <- character(0)
  flush <- function() {
    if (length(literal) > 0L) {
      parts[[length(parts) + 1L]] <<- list(
        kind = "text", value = paste(literal, collapse = "")
      )
      literal <<- character(0)
    }
  }
  pattern <- "^[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*$"
  index <- 1L
  end <- nchar(text)
  while (index <= end) {
    pair <- substr(text, index, min(index + 1L, end))
    if (pair %in% c("{{", "}}")) {
      literal <- c(literal, substr(text, index, index))
      index <- index + 2L
      next
    }
    character <- substr(text, index, index)
    if (character == "}") {
      .grammar_stop("unmatched closing brace", "invalid_string_template")
    }
    if (character != "{") {
      literal <- c(literal, character)
      index <- index + 1L
      next
    }
    remainder <- substr(text, index + 1L, end)
    closing <- regexpr("}", remainder, fixed = TRUE)
    if (closing < 0L) {
      .grammar_stop("unmatched opening brace", "invalid_string_template")
    }
    stop_index <- index + closing
    placeholder <- substr(text, index + 1L, stop_index - 1L)
    if (grepl("{", placeholder, fixed = TRUE) ||
      !grepl(pattern, placeholder)) {
      .grammar_stop(
        paste0("invalid placeholder '", placeholder, "'"),
        "invalid_string_template"
      )
    }
    flush()
    parts[[length(parts) + 1L]] <- list(
      kind = "placeholder", name = placeholder
    )
    index <- stop_index + 1L
  }
  flush()
  parts
}

#' Quote a literal for a shape the way R004 quotes a string
#' @noRd
.quote_grammar_scalar <- function(value) {
  paste0("'", gsub("'", "''", value, fixed = TRUE), "'")
}

#' @noRd
.predicate_operand_shape <- function(node) {
  if (node$kind == "identifier") {
    return(paste0("(id ", node$name, ")"))
  }
  if (is.na(node$type)) {
    return("null")
  }
  if (node$type %in% c("str", "date", "datetime")) {
    return(paste0(
      "(", node$type, " ", .quote_grammar_scalar(node$value), ")"
    ))
  }
  paste0("(", node$type, " ", node$value, ")")
}

#' Render a parsed predicate as the prefix form a vector records
#' @noRd
yamaa_predicate_shape <- function(node) {
  kind <- node$kind
  if (kind %in% c("and", "or")) {
    return(paste0(
      "(", kind, " ", yamaa_predicate_shape(node$left), " ",
      yamaa_predicate_shape(node$right), ")"
    ))
  }
  if (kind == "not") {
    return(paste0("(not ", yamaa_predicate_shape(node$value), ")"))
  }
  if (kind == "boolean") {
    return(if (node$value) "true" else "false")
  }
  if (kind == "comparison") {
    return(paste0(
      "(", node$operator, " ", .predicate_operand_shape(node$left), " ",
      .predicate_operand_shape(node$right), ")"
    ))
  }
  if (kind == "null_test") {
    name <- if (node$negated) "is-not-null" else "is-null"
    return(paste0(
      "(", name, " ", .predicate_operand_shape(node$value), ")"
    ))
  }
  if (kind == "in") {
    name <- if (node$negated) "not-in" else "in"
    operands <- vapply(node$values, .predicate_operand_shape, character(1))
    return(paste0(
      "(", name, " ", .predicate_operand_shape(node$value), " ",
      paste(operands, collapse = " "), ")"
    ))
  }
  if (kind == "between") {
    name <- if (node$negated) "not-between" else "between"
    return(paste0(
      "(", name, " ", .predicate_operand_shape(node$value), " ",
      .predicate_operand_shape(node$lower), " ",
      .predicate_operand_shape(node$upper), ")"
    ))
  }
  if (kind == "like") {
    name <- if (node$negated) "not-like" else "like"
    rendered <- paste0(
      "(", name, " ", .predicate_operand_shape(node$value), " ",
      .predicate_operand_shape(node$pattern)
    )
    if (!is.null(node$escape)) {
      rendered <- paste0(
        rendered, " (escape ", .quote_grammar_scalar(node$escape), ")"
      )
    }
    return(paste0(rendered, ")"))
  }
  stop("unknown predicate node kind: ", kind)
}

#' Render a parsed R010 or R013 expression as a vector's prefix form
#' @noRd
yamaa_expression_shape <- function(node) {
  kind <- node$kind
  if (kind == "number") {
    return(paste0("(", node$type, " ", node$value, ")"))
  }
  if (kind == "null") {
    return("null")
  }
  if (kind == "identifier") {
    return(paste0("(id ", node$name, ")"))
  }
  if (kind == "qualified_star") {
    return(paste0("(star ", node$dataset, ")"))
  }
  if (kind == "unary") {
    name <- if (node$operator == "-") "neg" else "pos"
    return(paste0("(", name, " ", yamaa_expression_shape(node$value), ")"))
  }
  if (kind == "binary") {
    return(paste0(
      "(", node$operator, " ", yamaa_expression_shape(node$left), " ",
      yamaa_expression_shape(node$right), ")"
    ))
  }
  if (kind == "reduction") {
    return(paste0(
      "(reduce ", toupper(node$name), " ",
      yamaa_expression_shape(node$argument), ")"
    ))
  }
  if (kind == "call") {
    rendered <- paste0("(call ", toupper(node$name))
    for (argument in node$arguments) {
      rendered <- paste0(rendered, " ", yamaa_expression_shape(argument))
    }
    return(paste0(rendered, ")"))
  }
  stop("unknown expression node kind: ", kind)
}

#' Render parsed template parts as the prefix form a vector records
#' @noRd
yamaa_string_template_shape <- function(parts) {
  rendered <- "(template"
  for (part in parts) {
    rendered <- if (part$kind == "text") {
      paste0(rendered, " (text ", .quote_grammar_scalar(part$value), ")")
    } else {
      paste0(rendered, " (placeholder ", part$name, ")")
    }
  }
  paste0(rendered, ")")
}

#' @noRd
.collect_identifiers <- function(node, names = character(0)) {
  if (!is.list(node)) {
    return(names)
  }
  if (identical(node$kind, "identifier")) {
    names <- c(names, node$name)
  }
  for (element in node) {
    if (!is.list(element)) {
      next
    }
    if (is.null(element$kind)) {
      for (item in element) {
        names <- .collect_identifiers(item, names)
      }
    } else {
      names <- .collect_identifiers(element, names)
    }
  }
  names
}

#' The identifier names R001 collects, ordered by code point
#' @noRd
.sorted_identifiers <- function(names) {
  sort(unique(names), method = "radix")
}

#' Decide one text under a closed grammar and its vocabulary
#'
#' Returns the failure condition, or the shape and identifiers an accepted
#' text produces. A vector pins what the grammar and its closed vocabulary
#' decide; which names are visible, and what they are typed, belongs to R001,
#' R002, and R007.
#' @noRd
yamaa_grammar_decision <- function(contract, text) {
  rejected <- function(condition_name) {
    list(
      condition = condition_name, shape = NA_character_,
      identifiers = character(0)
    )
  }
  accepted <- function(shape, names) {
    list(
      condition = NA_character_, shape = shape,
      identifiers = .sorted_identifiers(names)
    )
  }
  tryCatch(
    {
      if (contract == "predicate") {
        node <- yamaa_parse_predicate(text)
        accepted(yamaa_predicate_shape(node), .collect_identifiers(node))
      } else if (contract == "numeric") {
        node <- yamaa_parse_numeric_expression(text)
        conditions <- .numeric_conditions(node)
        if (length(conditions) > 0L) {
          rejected(conditions[1])
        } else {
          accepted(
            yamaa_expression_shape(node), .collect_identifiers(node)
          )
        }
      } else if (contract == "aggregate") {
        node <- yamaa_parse_aggregate_expression(text)
        conditions <- .aggregate_conditions(node)
        if (length(conditions) > 0L) {
          rejected(conditions[1])
        } else {
          accepted(
            yamaa_expression_shape(node), .collect_identifiers(node)
          )
        }
      } else if (contract == "string-template") {
        parts <- yamaa_parse_string_template(text)
        names <- vapply(
          Filter(function(part) part$kind == "placeholder", parts),
          function(part) part$name,
          character(1)
        )
        accepted(yamaa_string_template_shape(parts), names)
      } else {
        stop("unknown grammar contract: ", contract)
      }
    },
    yamaa_grammar_error = function(condition) {
      rejected(condition$condition_name)
    }
  )
}
