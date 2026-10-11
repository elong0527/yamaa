//! Compile decoded declarations into the same immutable model as original YAML.
//! Wire parsing, scalar codecs and source IO remain outside this pure service.
use super::*;
use crate::{
    bound_expression::Binding,
    function_signature::{InvocationPlan, Presence},
    numeric_compiler::{compile_numeric, CompileError, CompileLimits, CompiledNumeric},
};

/// Decoded declarations carry coordinates, never authority to execute a plan.
#[derive(Debug)]
pub struct DatasetDeclaration {
    pub sources: SourceSchemas,
    pub intermediates: Vec<Intermediate>,
    pub output: TableSchema,
    pub templates: Vec<TemplateDeclaration>,
    pub columns: Vec<AssignmentDeclaration>,
    pub keys: Vec<usize>,
    pub verifications: Vec<Verification>,
    pub conversion_handlers: Vec<ConversionHandler>,
    pub functions: Vec<InvocationPlan>,
}

#[derive(Debug)]
pub struct TemplateDeclaration {
    pub mode: RowMode,
    pub assignments: Vec<AssignmentDeclaration>,
    pub filter: Option<BoundPredicate>,
}

#[derive(Debug)]
pub struct AssignmentDeclaration {
    pub column: usize,
    pub path: String,
    pub expression: ExpressionDeclaration,
}

/// The temporary typed protocol's closed declaration vocabulary. Numeric text
/// and function references are compiled here, rather than admitted by a codec.
#[derive(Debug)]
pub enum ExpressionDeclaration {
    Literal(Value),
    Compute {
        text: String,
        bindings: Vec<Binding>,
    },
    Function {
        slot: usize,
        arguments: Vec<FunctionArgument>,
    },
    Number(Window),
    Window(Window),
    Source(usize),
    Column(usize),
    Collect {
        column: usize,
        identifier: String,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
    },
    Lookup(Lookup),
    RowLookup(RowLookup),
    Intermediate {
        index: usize,
        column: usize,
    },
    Reduce {
        identifier: Option<String>,
        column: usize,
        reducer: NumericReducer,
        text: String,
    },
    Count {
        column: Option<usize>,
        text: String,
    },
}

/// Operation tag decoded independently of the window's value-bearing fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowDeclarationKind {
    RowNumber,
    Competition,
    Dense,
    RowValue,
    PreviousNonMissing,
    Locf,
    BaselineFlag,
}
impl From<WindowKind> for WindowDeclarationKind {
    fn from(kind: WindowKind) -> Self {
        match kind {
            WindowKind::RowNumber => Self::RowNumber,
            WindowKind::Competition => Self::Competition,
            WindowKind::Dense => Self::Dense,
            WindowKind::RowValue { .. } => Self::RowValue,
            WindowKind::PreviousNonMissing { .. } => Self::PreviousNonMissing,
            WindowKind::Locf { .. } => Self::Locf,
            WindowKind::BaselineFlag { .. } => Self::BaselineFlag,
        }
    }
}
/// Preserve the legacy form gate before decoding subordinate window payloads.
pub fn validate_window_form(
    kind: WindowDeclarationKind,
    number: bool,
) -> Result<(), DatasetCompileError> {
    let numbering = matches!(
        kind,
        WindowDeclarationKind::RowNumber
            | WindowDeclarationKind::Competition
            | WindowDeclarationKind::Dense
    );
    if numbering == number {
        Ok(())
    } else {
        Err(PlanError::InvalidWindow.into())
    }
}

/// Trusted compilation policy; cumulative budgets precede signature copies.
#[derive(Clone, Copy, Debug)]
pub struct DatasetCompileLimits {
    pub functions: usize,
    pub function_arguments: usize,
    pub copied_parameters: usize,
    pub copied_text_bytes: usize,
    pub numeric_bindings: usize,
    pub numeric: CompileLimits,
}
impl Default for DatasetCompileLimits {
    fn default() -> Self {
        Self {
            functions: 64,
            function_arguments: 256,
            copied_parameters: 16_384,
            copied_text_bytes: 1_048_576,
            numeric_bindings: 4096,
            numeric: CompileLimits::default(),
        }
    }
}

#[derive(Debug)]
pub enum DatasetCompileError {
    Limit(&'static str),
    Numeric(CompileError),
    Plan(PlanError),
}
impl From<PlanError> for DatasetCompileError {
    fn from(error: PlanError) -> Self {
        Self::Plan(error)
    }
}

/// Only complete successful compilation grants access to the executable model.
#[derive(Debug)]
pub struct CompiledDataset {
    plan: DatasetPlan,
    functions: Vec<InvocationPlan>,
}
impl CompiledDataset {
    pub fn plan(&self) -> &DatasetPlan {
        &self.plan
    }
    pub fn functions(&self) -> &[InvocationPlan] {
        &self.functions
    }
}

/// Metadata completed after expression admission. It carries no table values.
#[derive(Debug)]
pub struct DatasetMetadata {
    pub sources: SourceSchemas,
    pub intermediates: Vec<Intermediate>,
    pub output: TableSchema,
    pub keys: Vec<usize>,
    pub verifications: Vec<Verification>,
    pub conversion_handlers: Vec<ConversionHandler>,
}

/// Compile and admit the full graph, including recovery, without table access,
/// callbacks, IO or host types. No partly admitted plan escapes on failure.
pub fn compile_dataset(
    declaration: DatasetDeclaration,
    limits: DatasetCompileLimits,
) -> Result<CompiledDataset, DatasetCompileError> {
    let mut compiler = DatasetCompiler::new(declaration.functions, limits)?;
    for template in declaration.templates {
        compiler.begin_template(template.mode, template.filter)?;
        for assignment in template.assignments {
            compiler.template_assignment(assignment)?;
        }
    }
    for assignment in declaration.columns {
        compiler.column_assignment(assignment)?;
    }
    compiler.finish(DatasetMetadata {
        sources: declaration.sources,
        intermediates: declaration.intermediates,
        output: declaration.output,
        keys: declaration.keys,
        verifications: declaration.verifications,
        conversion_handlers: declaration.conversion_handlers,
    })
}

/// Pure draft admission lets a codec preserve authored fault precedence while
/// retaining every executable expression and the final graph inside core.
/// Drafts expose no plan, expressions, callbacks or execution interface.
#[derive(Debug)]
pub struct DatasetCompiler {
    functions: Vec<InvocationPlan>,
    limits: DatasetCompileLimits,
    templates: Vec<RowTemplate>,
    columns: Vec<Assignment>,
    reserved_function: Option<(usize, InvocationPlan)>,
    reserved_numeric: Option<CompiledNumeric>,
    failed: bool,
}
impl DatasetCompiler {
    pub fn new(
        functions: Vec<InvocationPlan>,
        limits: DatasetCompileLimits,
    ) -> Result<Self, DatasetCompileError> {
        if functions.len() > limits.functions {
            return Err(DatasetCompileError::Limit("functions"));
        }
        Ok(Self {
            functions,
            limits,
            templates: Vec::new(),
            columns: Vec::new(),
            reserved_function: None,
            reserved_numeric: None,
            failed: false,
        })
    }
    pub fn begin_template(
        &mut self,
        mode: RowMode,
        filter: Option<BoundPredicate>,
    ) -> Result<(), DatasetCompileError> {
        self.ensure_live()?;
        self.templates.push(RowTemplate {
            mode,
            filter,
            assignments: Vec::new(),
        });
        Ok(())
    }
    pub fn template_assignment(
        &mut self,
        declaration: AssignmentDeclaration,
    ) -> Result<(), DatasetCompileError> {
        if self.templates.is_empty() {
            self.failed = true;
            return Err(PlanError::NoTemplates.into());
        }
        let assignment = self.assignment(declaration)?;
        self.templates
            .last_mut()
            .expect("template checked")
            .assignments
            .push(assignment);
        Ok(())
    }
    pub fn column_assignment(
        &mut self,
        declaration: AssignmentDeclaration,
    ) -> Result<(), DatasetCompileError> {
        let assignment = self.assignment(declaration)?;
        self.columns.push(assignment);
        Ok(())
    }
    /// Validate and charge a function slot before a codec decodes its arguments.
    /// The copied signature stays private and is consumed by the next function declaration.
    pub fn reserve_function(&mut self, slot: usize) -> Result<(), DatasetCompileError> {
        self.ensure_live()?;
        if self.reserved_function.is_some() || self.reserved_numeric.is_some() {
            self.failed = true;
            return Err(PlanError::InvalidFunction.into());
        }
        let signature = match self.copy_function(slot) {
            Ok(signature) => signature,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        self.reserved_function = Some((slot, signature));
        Ok(())
    }
    /// Compile grammar before decoding the numeric declaration's binding carrier.
    /// The tree stays private until that exact authored text/path receives bindings.
    pub fn reserve_numeric(&mut self, text: &str, path: &str) -> Result<(), DatasetCompileError> {
        self.ensure_live()?;
        if self.reserved_numeric.is_some() || self.reserved_function.is_some() {
            self.failed = true;
            return Err(PlanError::InvalidColumns.into());
        }
        match compile_numeric(text, path, self.limits.numeric) {
            Ok(compiled) => {
                self.reserved_numeric = Some(compiled);
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(DatasetCompileError::Numeric(error))
            }
        }
    }
    pub fn finish(self, metadata: DatasetMetadata) -> Result<CompiledDataset, DatasetCompileError> {
        self.ensure_live()?;
        if self.reserved_function.is_some() {
            return Err(PlanError::InvalidFunction.into());
        }
        if self.reserved_numeric.is_some() {
            return Err(PlanError::InvalidColumns.into());
        }
        let plan = DatasetPlan::new_with_intermediates(
            metadata.sources,
            metadata.intermediates,
            metadata.output,
            self.templates,
            self.columns,
            metadata.keys,
            metadata.verifications,
        )?
        .with_conversion_handlers(metadata.conversion_handlers)?;
        Ok(CompiledDataset {
            plan,
            functions: self.functions,
        })
    }
    fn assignment(
        &mut self,
        declaration: AssignmentDeclaration,
    ) -> Result<Assignment, DatasetCompileError> {
        self.ensure_live()?;
        let expression = match self.expression(declaration.expression, &declaration.path) {
            Ok(expression) => expression,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        Ok(Assignment {
            column: declaration.column,
            expression,
            path: declaration.path,
        })
    }

    fn ensure_live(&self) -> Result<(), DatasetCompileError> {
        if self.failed {
            Err(PlanError::InvalidColumns.into())
        } else {
            Ok(())
        }
    }

    fn copy_function(&mut self, slot: usize) -> Result<InvocationPlan, DatasetCompileError> {
        let signature = self.functions.get(slot).ok_or(PlanError::InvalidFunction)?;
        Self::charge(
            &mut self.limits.copied_parameters,
            signature.parameters().len(),
            "copied parameters",
        )?;
        let identity = signature.identity();
        for text in [
            &identity.name,
            &identity.contract_version,
            &identity.implementation_version,
            &identity.call,
        ] {
            Self::charge(
                &mut self.limits.copied_text_bytes,
                text.len(),
                "copied text",
            )?;
        }
        for parameter in signature.parameters() {
            for text in [&parameter.name, &parameter.host_name] {
                Self::charge(
                    &mut self.limits.copied_text_bytes,
                    text.len(),
                    "copied text",
                )?;
            }
            if let Presence::Optional(Value::Str(text)) = &parameter.presence {
                Self::charge(
                    &mut self.limits.copied_text_bytes,
                    text.len(),
                    "copied text",
                )?;
            }
        }
        Ok(signature.clone())
    }

    fn expression(
        &mut self,
        declaration: ExpressionDeclaration,
        path: &str,
    ) -> Result<Expression, DatasetCompileError> {
        use ExpressionDeclaration as D;
        Ok(match declaration {
            D::Literal(value) => Expression::Literal(value),
            D::Compute { text, bindings } => {
                if bindings.len() > self.limits.numeric_bindings {
                    return Err(DatasetCompileError::Limit("numeric bindings"));
                }
                let compiled = match self.reserved_numeric.take() {
                    Some(compiled)
                        if compiled.expression() == text && compiled.spec_path() == path =>
                    {
                        compiled
                    }
                    Some(_) => return Err(PlanError::InvalidColumns.into()),
                    None => compile_numeric(&text, path, self.limits.numeric)
                        .map_err(DatasetCompileError::Numeric)?,
                };
                Expression::Compute(
                    BoundNumeric::new(compiled, bindings).map_err(|_| PlanError::InvalidColumns)?,
                )
            }
            D::Function { slot, arguments } => {
                if arguments.len() > self.limits.function_arguments {
                    return Err(DatasetCompileError::Limit("function arguments"));
                }
                let signature = match self.reserved_function.take() {
                    Some((reserved, signature)) if reserved == slot => signature,
                    Some(_) => return Err(PlanError::InvalidFunction.into()),
                    None => self.copy_function(slot)?,
                };
                Expression::Function(BoundFunction::new(slot, signature, arguments)?)
            }
            D::Number(window) => {
                validate_window_form(window.kind.into(), true)?;
                Expression::Window(window)
            }
            D::Window(window) => {
                validate_window_form(window.kind.into(), false)?;
                Expression::Window(window)
            }
            D::Source(column) => Expression::Source(column),
            D::Column(column) => Expression::Column(column),
            D::Collect {
                column,
                identifier,
                filter,
                selection,
            } => Expression::Collect {
                column,
                identifier,
                filter,
                selection,
            },
            D::Lookup(lookup) => Expression::Lookup(lookup),
            D::RowLookup(lookup) => Expression::RowLookup(lookup),
            D::Intermediate { index, column } => Expression::Intermediate { index, column },
            D::Reduce {
                identifier,
                column,
                reducer,
                text,
            } => Expression::Reduce {
                identifier,
                column,
                reducer,
                text,
            },
            D::Count { column, text } => Expression::Count { column, text },
        })
    }

    fn charge(
        remaining: &mut usize,
        amount: usize,
        resource: &'static str,
    ) -> Result<(), DatasetCompileError> {
        *remaining = remaining
            .checked_sub(amount)
            .ok_or(DatasetCompileError::Limit(resource))?;
        Ok(())
    }
}
