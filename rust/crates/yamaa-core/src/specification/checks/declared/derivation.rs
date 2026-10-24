//! One derivation, node by node, as binding binds it: each read of the
//! implicit template, a row template or the column phase, a structured
//! source's declaration and selection, a window's grouping and defects,
//! and the static types binding checks once a derivation's reads bind.
use super::*;

impl Checks<'_> {
    /// The record selections at the positions below `root`, in binding's
    /// order: each source binding's declaration and filter, a mapping or
    /// cut's source type, each window's, each aggregate's header and filter,
    /// and each ODM read's filter. Without row templates (`scope`), also each
    /// read of another input through its implicit join ([`Self::join`]), a
    /// `case` predicate's before its result, and the static types binding
    /// checks once those reads bind: each `case` predicate's, a flag's once
    /// every name it reads binds, and a `case` result formula's operands'.
    /// `output` types a column window's baseline operands, which only a spec
    /// without row templates does here.
    fn selections(
        &self,
        root: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        self.select(root, scope, output, false, findings)
    }

    /// [`Self::selections`] at the position `e`, a `case` result when
    /// `branch`.
    fn select(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        branch: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        match &spec.expr(e).kind {
            // The implicit template types each predicate once its names are
            // read, before its result (`case_predicates::implicit_predicate`).
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let (Some(scope), Some(condition)) = (scope, arm.condition) {
                        self.predicate_joins(scope, condition, findings);
                        condition_types(spec, condition, |name| self.declared_type(name), findings);
                    }
                    self.select(arm.result, scope, output, true, findings)?;
                }
            }
            ExprKind::Ref(read) => {
                if let Some(scope) = scope {
                    self.read(scope, &path(spec, e), read.as_str(), findings);
                }
            }
            ExprKind::Source(_) => self.selection(e, scope, false, findings)?,
            ExprKind::FirstAvailable { sources, .. } => {
                for &source in sources {
                    self.selection(source, scope, branch, findings)?;
                }
            }
            // Each operand is read where binding reads it, a source binding
            // through its selection; the verb's operand types once a source
            // binding reads without a finding (EXPR-11, EXPR-13), where
            // `check()` types them ([`CheckTyping::Always`]). Preflight typed
            // every other name a declaration types, and refused the spec when
            // it refused one or the verb's late refusal followed; binding
            // takes a lookup's field untyped, so the late refusal follows
            // its read here (TEXT-6, TEXT-10).
            ExprKind::Scalar { op, .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?;
                let mut taken = scope.is_some();
                for (index, operand) in operation.operands().iter().enumerate() {
                    match &operand.read {
                        Read::Source(source) => {
                            let start = findings.len();
                            self.selection(*source, scope, false, findings)?;
                            let typed = findings.len() == start
                                && scope.is_some()
                                && op.check_typing() == CheckTyping::Always;
                            if let Some(read) = variable(spec, *source).filter(|_| typed) {
                                self.operand_type(&operation, index, read, findings);
                            }
                            taken = false;
                        }
                        Read::Name(read) => {
                            if let Some(scope) = scope {
                                taken &= self.read(scope, &operand.path, read, findings)
                                    && untyped(&operation, self.lookup_field(read));
                            }
                        }
                        Read::Literal(_) => {}
                    }
                }
                late_refusal(&operation, taken, findings);
            }
            // A flag is typed only once every name it reads binds
            // (`flags::bind`).
            ExprKind::Flag { condition, .. } => {
                if let Some(scope) = scope {
                    let start = findings.len();
                    self.predicate_joins(scope, *condition, findings);
                    if findings.len() == start {
                        condition_types(
                            spec,
                            *condition,
                            |name| self.declared_type(name),
                            findings,
                        );
                    }
                }
            }
            ExprKind::Window(window) => {
                self.window(e, window, scope, findings)?;
                if output {
                    self.baseline_types(window, findings);
                }
            }
            ExprKind::Aggregate(aggregate) => {
                self.reduction(e, aggregate, scope, findings)?;
            }
            ExprKind::Odm(read) => {
                if let Some(filter) = read.filter {
                    self.grammar(filter, findings)?;
                }
            }
            ExprKind::Call { args, .. } => {
                if let Some(scope) = scope {
                    for &(_, value) in args {
                        if let Some(read) = name(spec, value) {
                            self.join(scope, &path(spec, value), read, findings);
                        }
                    }
                }
            }
            ExprKind::Formula(ast::Formula {
                compiled: Ok(compiled),
                text,
                ..
            }) if branch && scope.is_some() => {
                self.branch_formula(compiled, &render(spec, text.meta), findings);
            }
            ExprKind::Literal(_) | ExprKind::Formula(_) | ExprKind::Unsupported(_) => {}
        }
        Ok(())
    }

    /// Without row templates, a read at `path` of another input through its
    /// implicit join, which matches on the input's declared `key`: an input that declares none fails at the read, whatever it
    /// stores (LOOKUP-5, D111), and so does one whose key column the current
    /// row cannot pair ([`declared_key`]). True when the read binds, as far
    /// as this decides.
    fn join(
        &self,
        scope: Scope<'_>,
        path: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let Some((owner, _)) = name.split_once('.') else {
            return true;
        };
        if scope.drivers.contains(&owner) || !reads_as_input(self.prepared, owner) {
            return true;
        }
        let refused = if keyless(self.spec, owner) {
            Some(undeclared_key(self.spec, path, owner))
        } else {
            declared_key(
                self.spec,
                self.prepared,
                &self.declared,
                path,
                owner,
                Pairing::Column,
            )
        };
        let Some(finding) = refused else {
            return true;
        };
        findings.push(finding);
        false
    }

    /// Whether `name` is a lookup's field, which the implicit template
    /// reads through the lookup's selector.
    fn lookup_field(&self, name: &str) -> bool {
        name.split_once('.')
            .is_some_and(|(owner, _)| self.prepared.intermediates.contains(owner))
    }

    /// Without row templates, a read at `path`: of a lookup binding does not
    /// bind, or of a field qualified by nothing declared, each of which fails
    /// as unknown (NAME-3), or of another input through its implicit join
    /// ([`Self::join`]). True when it binds, as far as this decides.
    fn read(
        &self,
        scope: Scope<'_>,
        path: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        if self.refused_lookup(name)
            || name
                .split_once('.')
                .is_some_and(|(owner, _)| self.undeclared(owner) && !scope.drivers.contains(&owner))
        {
            findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.into(),
            });
            return false;
        }
        self.join(scope, path, name, findings)
    }

    /// Without row templates, a formula in a `case` result, at its text
    /// `path`, reads only lookups and columns (`BindContext::branch_formula`):
    /// each name, once, that is no column, no lookup's field, or a field of a
    /// lookup binding does not bind fails as unknown (NAME-3). Then its
    /// operands are typed over the names that bound ([`Self::column_type`],
    /// FORMULA-12).
    fn branch_formula(
        &self,
        compiled: &CompiledExpression,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) {
        for name in compiled.identifiers() {
            let unknown = match name.split_once('.') {
                Some((owner, _)) if self.prepared.intermediates.contains(owner) => {
                    self.refused_lookup(name)
                }
                Some(_) => true,
                None => !self
                    .spec
                    .columns
                    .iter()
                    .any(|column| column.name.name == name),
            };
            if unknown {
                findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.into(),
                });
            }
        }
        operand_types(compiled, |name| self.column_type(name), findings);
    }

    /// Without row templates, each name a flag's or `case` predicate's
    /// condition reads, once, which a name that binding cannot read there
    /// leaves unknown to the predicate (FORMULA-9): one qualified by nothing
    /// an input or a lookup is, a field of a lookup binding does not bind,
    /// or a read of another input through its implicit join that fails
    /// ([`Self::join`]), whose refusal comes first.
    fn predicate_joins(
        &self,
        scope: Scope<'_>,
        condition: ExprId,
        findings: &mut Vec<BindFinding>,
    ) {
        for (at, read) in names(self.spec, condition) {
            let undeclared = read
                .split_once('.')
                .is_some_and(|(owner, _)| self.undeclared(owner));
            if undeclared || self.refused_lookup(read) || !self.join(scope, &at, read, findings) {
                findings.push(predicate_unknown(&at, read));
            }
        }
    }

    /// A source operand's record selection ([`Self::source`]); then, without
    /// row templates, `require_match` and `between` on a read that binds and
    /// is no implicit join ([`join_options`]). Whatever its filter reports,
    /// the implicit template binds a field of the base, a column, or a field
    /// of a lookup whose key types do not fail, and reads another input
    /// through its implicit join ([`Self::join`]). A `first_available` source of a
    /// `case` result (`branch`) binds as a row template's does, only with no
    /// finding of its own.
    fn selection(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        branch: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let start = findings.len();
        // A name, in either rules, reads another input through its join.
        if let (ExprKind::Ref(read), Some(scope)) = (&spec.expr(e).kind, scope) {
            self.read(scope, &path(spec, e), read.as_str(), findings);
            return Ok(());
        }
        let (false, Some(scope), ExprKind::Source(source)) =
            (self.source(e, scope, findings)?, scope, &spec.expr(e).kind)
        else {
            return Ok(());
        };
        if branch {
            super::boundaries::Reads::column(
                spec,
                self.prepared,
                &self.declared,
                scope.drivers,
                true,
                &[],
                &self.refused,
            )
            .source(e, start, findings);
            return Ok(());
        }
        let variable = name(spec, source.variable).unwrap_or_default();
        if !self.read(scope, &path(spec, e), variable, findings) {
            return Ok(());
        }
        // A join that binds keeps its range, whose value reads the current
        // row; a lookup binding does not bind gives it none, nor does a
        // record nothing declared names (LOOKUP-9).
        if let Some((between, value)) = source
            .between
            .as_ref()
            .and_then(|between| Some((between, name(spec, between.value)?)))
            .filter(|_| {
                variable.split_once('.').is_some_and(|(owner, _)| {
                    !scope.drivers.contains(&owner) && reads_as_input(self.prepared, owner)
                })
            })
            .filter(|&(_, value)| {
                self.refused_lookup(value)
                    || value
                        .split_once('.')
                        .is_some_and(|(owner, _)| self.undeclared(owner))
            })
        {
            findings.push(unknown_range_value(
                spec,
                between,
                Owner::Source(variable),
                value,
            ));
            return Ok(());
        }
        // A read of another input through its implicit join binds its range.
        if variable.split_once('.').is_some_and(|(owner, _)| {
            !scope.drivers.contains(&owner) && reads_as_input(self.prepared, owner)
        }) {
            findings.extend(join_range_findings(spec, self.prepared, e));
            return Ok(());
        }
        let binds = match variable.split_once('.') {
            Some((owner, _)) => match spec
                .intermediates
                .iter()
                .position(|item| item.id.name == owner)
            {
                Some(index) => !self.refused.contains(&index),
                None => scope.drivers.contains(&owner),
            },
            None => spec
                .columns
                .iter()
                .any(|column| column.name.name == variable),
        };
        if binds {
            join_options(spec, e, findings);
        }
        Ok(())
    }

    /// One derivation of a row template or the column phase, node by node
    /// in binding's order (`BindContext::bind`): a structured source's
    /// declaration and selection before its variable, and its join options
    /// after it, a window's grouping and defects before its reads, a
    /// mapping's source type and a placeholder's type once their read binds,
    /// an aggregate's header alone, and each other read where binding binds
    /// it ([`super::boundaries`]). The authored defects precede it.
    pub(super) fn walk(
        &self,
        reads: &super::boundaries::Reads<'_>,
        scope: Scope<'_>,
        e: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let names = |text: ExprId| names(spec, text);
        // A source operand: a structured binding unless its declaration or
        // selection stops binding before its variable, or a name. True when
        // it binds.
        let source = |e: ExprId, findings: &mut Vec<BindFinding>| -> Result<bool, BindError> {
            Ok(match &spec.expr(e).kind {
                ExprKind::Source(_) => {
                    let start = findings.len();
                    !self.source(e, Some(scope), findings)? && reads.source(e, start, findings)
                }
                ExprKind::Ref(read) => reads.read(&path(spec, e), read.as_str(), findings),
                _ => true,
            })
        };
        match &spec.expr(e).kind {
            ExprKind::Literal(_) | ExprKind::Unsupported(_) => {}
            ExprKind::Ref(_) | ExprKind::Source(_) => {
                source(e, findings)?;
            }
            // Each name, then the operand types over the names that bound: a
            // column's or a stored field's declared type (FORMULA-12).
            ExprKind::Formula(formula) => {
                if let Ok(compiled) = &formula.compiled {
                    let mut bound = Vec::new();
                    for (at, read) in names(e) {
                        if reads.formula(&at, compiled.expression(), read, findings) {
                            bound.push(read);
                        }
                    }
                    operand_types(
                        compiled,
                        |name| {
                            bound
                                .contains(&name)
                                .then(|| self.declared.name(name).single())
                                .flatten()
                        },
                        findings,
                    );
                }
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    for (at, read) in arm.condition.map(names).unwrap_or_default() {
                        reads.predicate(&at, read, findings);
                    }
                    self.walk(reads, scope, arm.result, findings)?;
                }
            }
            // Each name, then the condition's types once every name binds
            // (`flags::bind`).
            ExprKind::Flag { condition, .. } => {
                let start = findings.len();
                for (at, read) in names(*condition) {
                    reads.flag(&at, read, findings);
                }
                if findings.len() == start {
                    condition_types(spec, *condition, |name| self.declared_type(name), findings);
                }
            }
            ExprKind::FirstAvailable { sources, .. } => {
                for &operand in sources {
                    source(operand, findings)?;
                }
            }
            // Each operand is read where binding reads it, a source binding
            // as binding binds it; once a read binds, the verb's operand type
            // where `check()` types it (EXPR-11, EXPR-13, TEXT-1); and once
            // binding takes every operand, the verb's late refusal (TEXT-6,
            // TEXT-10).
            ExprKind::Scalar { op, .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?;
                let mut taken = true;
                for (index, operand) in operation.operands().iter().enumerate() {
                    let (read, bound) = match &operand.read {
                        Read::Source(node) => (variable(spec, *node), source(*node, findings)?),
                        Read::Name(name) => (
                            Some(name.as_str()),
                            reads.read(&operand.path, name, findings),
                        ),
                        Read::Literal(_) => continue,
                    };
                    let Some(read) = read.filter(|_| bound) else {
                        taken = false;
                        continue;
                    };
                    let typed = (op.check_typing() != CheckTyping::Never)
                        .then(|| self.operand_type(&operation, index, read, findings))
                        .flatten();
                    taken &= typed.unwrap_or_else(|| untyped(&operation, reads.lookup_field(read)));
                }
                late_refusal(&operation, taken, findings);
            }
            ExprKind::Window(window) => self.window(e, window, Some(scope), findings)?,
            ExprKind::Aggregate(aggregate) => {
                self.reduction(e, aggregate, Some(scope), findings)?
            }
            ExprKind::Odm(read) => {
                if let Some(filter) = read.filter {
                    self.grammar(filter, findings)?;
                }
            }
            ExprKind::Call { args, .. } => {
                for &(_, value) in args {
                    if let Some(read) = name(spec, value) {
                        reads.argument(&path(spec, value), read, findings);
                    }
                }
            }
        }
        Ok(())
    }

    /// One derivation's authored defects, then its record selections.
    pub(super) fn derivation(
        &self,
        root: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        self.defects(root, findings)?;
        self.selections(root, scope, output, findings)
    }

    /// A structured source binding's declaration defects, reported before
    /// any field is bound: a range without a bound, an unpaired selection
    /// and two absence policies; then a filter on one value (a group key, an
    /// output column, a row's value or a named intermediate's field), the
    /// filter's grammar and each name it reads outside its variable's input,
    /// and, outside the implicit template, each name of a variable's record
    /// that nothing declares, unless the variable reads a lookup binding does
    /// not bind, whose read fails first. True when binding binds nothing more
    /// of it: a declaration defect, or a filter on a group key.
    pub(super) fn source(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        let spec = self.spec;
        let ExprKind::Source(source) = &spec.expr(e).kind else {
            return Ok(false);
        };
        let path = path(spec, e);
        let variable = name(spec, source.variable).unwrap_or_default();
        let owner = Owner::Source(variable);
        let mut defects = Vec::new();
        if let Some(between) = &source.between {
            if between.lower.is_none() && between.upper.is_none() {
                defects.push(lookup::range_declaration(
                    &render(spec, between.meta),
                    owner,
                ));
            }
        }
        if source.order_by.is_empty() != source.keep.is_none() {
            let (declared, missing) = if source.keep.is_some() {
                ("keep", "order_by")
            } else {
                ("order_by", "keep")
            };
            defects.push(lookup::selection_unpaired(
                &path,
                owner,
                &[declared],
                &[missing],
            ));
        }
        if source.require_match && source.absent.is_some() {
            defects.push(lookup::absence_conflict(
                format!("{path}.absent"),
                format!("{path}.require_match"),
                owner,
                source_expressions::source_dataset(variable),
            ));
        }
        // A declared grouped key is already one scalar in both row and column
        // phase. A row-phase read's driver is its row template's own; the
        // column phase decides from the groups alone.
        if let (
            Some(filter),
            Some(Scope {
                drivers,
                phase: Some(phase),
                ..
            }),
        ) = (source.filter, scope)
        {
            let driver = drivers.first().copied().unwrap_or_default();
            if defects.is_empty() && group_key(variable, driver, phase) {
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Err(BindError::Internal);
                };
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    C::SourceFilterSingleValue,
                    &render(spec, formula.text.meta),
                    &[("identifier", variable), ("construct", "filter")],
                )));
                return Ok(true);
            }
        }
        if !defects.is_empty() {
            findings.extend(defects.into_iter().map(|defect| {
                BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(defect))
            }));
            return Ok(true);
        }
        // Binding reads a lookup it does not bind before it binds a filter.
        let Some(filter) = source.filter.filter(|_| !self.refused_lookup(variable)) else {
            return Ok(false);
        };
        let qualifier = variable.split_once('.').map(|(owner, _)| owner);
        match (
            qualifier.filter(|owner| !self.prepared.intermediates.contains(owner)),
            scope,
        ) {
            // Only a stored variable's filter is compiled. It reads only the
            // records of its variable's input, so binding refuses each other
            // name before it binds a field. A variable qualified by nothing
            // declared stores no field: under row templates and over a donor's
            // records each name qualified as it is is unknown there too
            // (NAME-3, `Declaration::bind_row_filter`), and the implicit
            // template refuses the variable before it binds any filter.
            (Some(dataset), scope) => {
                let undeclared = self.undeclared(dataset)
                    && scope.is_none_or(|scope| !scope.drivers.contains(&dataset));
                if undeclared && scope.is_some_and(|scope| scope.phase.is_none()) {
                    return Ok(false);
                }
                let start = findings.len();
                self.grammar(filter, findings)?;
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Ok(false);
                };
                let (Some(_), true, Ok(plan)) = (
                    scope,
                    findings.len() == start
                        && (undeclared || reads_as_input(self.prepared, dataset)),
                    &formula.compiled,
                ) else {
                    return Ok(false);
                };
                let at = render(spec, formula.text.meta);
                let mut seen = BTreeSet::new();
                for identifier in plan.identifiers() {
                    if !seen.insert(identifier) {
                        continue;
                    }
                    if identifier
                        .split_once('.')
                        .is_none_or(|(owner, _)| owner != dataset)
                    {
                        findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                            C::SourceFilterReference,
                            &at,
                            &[("identifier", identifier), ("dataset", dataset)],
                        )));
                    } else if undeclared {
                        findings.push(BindFinding::UnknownReference {
                            path: at.clone(),
                            name: identifier.into(),
                        });
                    }
                }
            }
            // An output column, a row's value or a named intermediate's
            // field is one value already: there are no records to filter.
            // Without row templates a bare name that is no output column
            // fails as a reference first (D67).
            (None, Some(scope))
                if qualifier.is_some()
                    || scope.phase.is_some()
                    || spec
                        .columns
                        .iter()
                        .any(|column| column.name.name == variable) =>
            {
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Err(BindError::Internal);
                };
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    C::SourceFilterSingleValue,
                    &render(spec, formula.text.meta),
                    &[("identifier", variable), ("construct", "filter")],
                )));
            }
            (None, _) => {}
        }
        Ok(false)
    }

    /// The findings binding reports for a window that the declarations
    /// decide, in binding's order: the grouping refusals, the authored
    /// defects, then each read of a dataset other than the rows' driver.
    /// Without a scope only the authored defects.
    pub(super) fn window(
        &self,
        e: ExprId,
        window: &ast::Window,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let path = path(spec, e);
        let operands = window_operands(spec, window);
        let driver = |name: &str, scope: Scope<'_>| {
            name.split_once('.')
                .is_some_and(|(qualifier, _)| scope.drivers.contains(&qualifier))
        };
        // Under grouped construction each operand naming a field of the rows'
        // driver must be a group key (ROW-5 in a row template, NAME-4 at
        // column level). Binding decides it only for a field the driver
        // stores, and reports any other as unknown; without source schemas
        // every such field is stored.
        if let Some((scope, phase)) = scope.and_then(|scope| Some((scope, scope.phase?))) {
            for (name, path) in operands.iter().filter(|(name, _)| driver(name, scope)) {
                if let Some(finding) = crate::reference_scope::group_finding(name, phase) {
                    findings.push(BindFinding::QualifiedReference {
                        path: path.clone(),
                        name: (*name).into(),
                        row: scope.row.map(String::from),
                        finding,
                    });
                }
            }
        }
        let (operation, baseline) = match &window.function {
            WindowFunction::RowNumber => ("row_number", false),
            WindowFunction::Rank { .. } => ("rank", false),
            WindowFunction::RowValue { offset } => {
                if offset.parse::<i64>() == Ok(0) {
                    findings.push(BindFinding::Window(WindowFinding::ZeroOffset {
                        path: format!("{path}.offset"),
                    }));
                }
                ("row_value", false)
            }
            WindowFunction::Locf { .. } => ("locf", false),
            WindowFunction::BaselineFlag { .. } => ("baseline_flag", true),
        };
        // A positional window needs an order; a baseline orders by its date
        // and tie-break terms (WINDOW-5).
        if !baseline && window_order(window).is_empty() {
            findings.push(BindFinding::Window(WindowFinding::Order {
                path: format!("{path}.window"),
                operation: operation.into(),
            }));
        }
        // A partition names each variable once (WINDOW-3); report every repeat.
        for (index, &group) in window.group_by.iter().enumerate() {
            let named = name(spec, group).unwrap_or_default();
            if window.group_by[..index]
                .iter()
                .any(|&prior| name(spec, prior).unwrap_or_default() == named)
            {
                findings.push(BindFinding::Window(WindowFinding::RepeatedGroup {
                    path: super::path(spec, group),
                    identifier: named.into(),
                }));
            }
        }
        if let Some(filter) = window.filter {
            self.grammar(filter, findings)?;
        }
        // Binding's catalog holds one dataset, the rows' driver, so it refuses
        // a field of any other dataset whether or not that dataset stores it
        // (NAME-3), and a row template's window reads only the columns the
        // template derives in the row phase (COLUMN-4): the operands, then
        // each filter name once, in sorted order.
        if let Some(scope) = scope {
            let filtered = window
                .filter
                .and_then(|filter| match &spec.expr(filter).kind {
                    ExprKind::Formula(ast::Formula {
                        compiled: Ok(plan),
                        text,
                        ..
                    }) => Some((plan, render(spec, text.meta))),
                    _ => None,
                })
                .into_iter()
                .flat_map(|(plan, path)| {
                    let names = plan.identifiers().collect::<BTreeSet<_>>();
                    names.into_iter().map(move |name| (name, path.clone()))
                });
            for (name, path) in operands.into_iter().chain(filtered) {
                if name.contains('.') {
                    if !driver(name, scope) {
                        findings.push(BindFinding::UnknownReference {
                            path,
                            name: name.into(),
                        });
                    }
                } else if let (Some(row), Some(column)) = (
                    scope.row.filter(|_| scope.available.is_some()),
                    spec.columns
                        .iter()
                        .position(|column| column.name.name == name)
                        .filter(|column| {
                            scope
                                .available
                                .is_some_and(|available| !available.contains(column))
                        }),
                ) {
                    findings.push(BindFinding::OutputReference {
                        path,
                        name: name.into(),
                        row: Some(row.into()),
                        finding: crate::reference_binding::Diagnostic::PhaseBoundary { column },
                    });
                }
            }
        }
        Ok(())
    }

    /// A column window's baseline whose operands are all bare output columns
    /// has their declared types: its date is a date or datetime, and its
    /// reference date has the date's type (WINDOW-9). Binding decides every
    /// other read.
    fn baseline_types(&self, window: &ast::Window, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        let WindowFunction::BaselineFlag {
            window_order_by, ..
        } = &window.function
        else {
            return;
        };
        let output = |name: &str| spec.columns.iter().find(|column| column.name.name == name);
        let kind = |e: ExprId| name(spec, e).and_then(output).map(|column| column.kind);
        let operand = |slot| {
            window
                .operands
                .iter()
                .find(|operand| operand.slot == slot)
                .map(|operand| operand.expr)
        };
        let (Some(value), Some(reference), Some((date, tie_break))) = (
            operand(Slot::Source),
            operand(Slot::Reference),
            window.order_by.split_first(),
        ) else {
            return;
        };
        let groups = window
            .group_by
            .iter()
            .map(|&group| name(spec, group))
            .collect::<Vec<_>>();
        let filtered = window.filter.is_none_or(|filter| {
            matches!(&spec.expr(filter).kind, ExprKind::Formula(ast::Formula {
                compiled: Ok(plan),
                ..
            }) if plan.identifiers().all(|name| output(name).is_some()))
        });
        if !window_order_by.is_empty()
            || !filtered
            || kind(value).is_none()
            || tie_break.iter().any(|term| kind(term.variable).is_none())
            || window.group_by.iter().enumerate().any(|(index, &group)| {
                kind(group).is_none() || groups[..index].contains(&groups[index])
            })
        {
            return;
        }
        let (Some(date_kind), Some(reference_kind)) = (kind(date.variable), kind(reference)) else {
            return;
        };
        let (operand, source, expected, actual) =
            if !matches!(date_kind, ColumnType::Date | ColumnType::DateTime) {
                ("date", date.variable, "date or datetime", date_kind)
            } else if date_kind != reference_kind {
                (
                    "reference_date",
                    reference,
                    type_name(date_kind),
                    reference_kind,
                )
            } else {
                return;
            };
        // An operand's path is the field it is written in (P-OP-2).
        let at = if operand == "date" {
            spec.cite(date.meta)
        } else {
            spec.cite(spec.expr(reference).meta)
        };
        let mut finding = LookupFinding::new(&at, C::WindowBaselineInputType);
        for (field, value) in [
            ("operation", "baseline_flag"),
            ("source", name(spec, source).unwrap_or_default()),
            ("expected", expected),
            ("actual", type_name(actual)),
        ] {
            finding.text(field, value);
        }
        findings.push(BindFinding::Scalar(finding.diagnostic()));
    }

    /// Under row templates, the static types over the declared types of each
    /// `case` predicate at the positions below `root` (FORMULA-5,
    /// FORMULA-12), which binding finds once it evaluates the predicate of a
    /// derivation that bound. The walk types a flag where binding does, and
    /// without row templates a `case` predicate too ([`Self::selections`]).
    pub(super) fn predicate_types(&self, root: ExprId, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        if let ExprKind::Case(arms) = &spec.expr(root).kind {
            for arm in arms {
                if let Some(condition) = arm.condition {
                    condition_types(spec, condition, |name| self.declared_type(name), findings);
                }
                self.predicate_types(arm.result, findings);
            }
        }
    }
}
