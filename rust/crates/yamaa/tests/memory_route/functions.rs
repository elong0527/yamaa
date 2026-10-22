//! The benchmarks' project functions as a Rust port that claims R.
//!
//! The function benchmarks call six functions of an installed R package,
//! `yamaabenchmarks` (`rust/tests/project-programs/r`), which every R
//! environment file names and every renv.lock pins. [`Benchmarks`] is a
//! [`ProjectFunctions`] port that does what the R host does with that
//! package, in Rust:
//!
//! - it verifies the lock as `.yamaa_verify_locked_versions` does: `yamaa` at
//!   the engine's version and the package at the version ported here;
//! - it binds a `package::function` call as `.yamaa_resolve_locked_function`
//!   does, refusing a callable whose formals differ from the declared
//!   parameters;
//! - it calls each function with the plain R values the R binding passes
//!   (`int` and `float` as doubles, `bool` as a logical, missing as `NA`) and
//!   converts the result by the declared type, as the R binding does, so a
//!   non-finite double is missing.
//!
//! Each function follows its R source operation by operation, so it returns
//! the same binary64 values. The engine runs every contract test an
//! environment file declares through this port when it activates the
//! functions, before any study data is read: a port that strays from a
//! declared result fails the build.

use std::collections::BTreeSet;
use std::f64::consts::{FRAC_2_SQRT_PI, LN_2, SQRT_2};

use yamaa::{
    Argument, CallError, ColumnType, FunctionError, FunctionHandle, FunctionIdentity, HostLanguage,
    LockContext, LockFinding, LockKind, LockReason, ProjectFunctions, Scalar, ScalarRef, Signature,
};

/// The package the R environment files call, and the version this port
/// implements (`rust/tests/project-programs/r/DESCRIPTION`).
pub const PACKAGE: &str = "yamaabenchmarks";
pub const PACKAGE_VERSION: &str = "1.0.0";

/// A plain R value of length one, as a project function receives and
/// returns it.
#[derive(Clone, Debug, PartialEq)]
pub enum Plain {
    Na,
    Double(f64),
    Logical(bool),
    Character(String),
}

/// One formal of a function: its name and its R default, if it has one.
type Formal = (&'static str, Option<fn() -> Plain>);

/// One exported function: its name, its formals in order with their R
/// defaults, and its body over the values bound to them.
#[derive(Clone, Copy)]
pub struct Function {
    pub name: &'static str,
    formals: &'static [Formal],
    body: fn(&[Plain]) -> Result<Plain, String>,
}

impl Function {
    /// The formal names, in order.
    pub fn formals(&self) -> impl Iterator<Item = &'static str> {
        self.formals.iter().map(|(name, _)| *name)
    }

    /// Call the function with named arguments, as `do.call` does: a formal
    /// without an argument takes its default, and one without a default is
    /// an error, as is an argument no formal names. Every body reads every
    /// formal, so erring before the body runs is what R's lazy evaluation
    /// does too.
    pub fn call(&self, arguments: &[(&str, Plain)]) -> Result<Plain, String> {
        let mut bound = Vec::with_capacity(self.formals.len());
        for (formal, default) in self.formals {
            let value = arguments
                .iter()
                .find(|(name, _)| name == formal)
                .map(|(_, value)| value.clone())
                .or_else(|| default.map(|default| default()));
            bound.push(
                value
                    .ok_or_else(|| format!("argument \"{formal}\" is missing, with no default"))?,
            );
        }
        if let Some((name, _)) = arguments
            .iter()
            .find(|(name, _)| !self.formals().any(|formal| formal == *name))
        {
            return Err(format!("unused argument ({name})"));
        }
        (self.body)(&bound)
    }
}

/// The exported functions of the package (its NAMESPACE).
pub const FUNCTIONS: [Function; 6] = [
    Function {
        name: "bmi",
        formals: &[
            ("weight_kg", None),
            ("height_cm", None),
            ("cm_per_m", Some(|| Plain::Double(100.0))),
        ],
        body: |values| {
            // weight_kg / (height_cm / as.integer(cm_per_m)) ^ 2, where R
            // computes `^ 2` as one multiplication.
            let scale = match as_integer(&values[2])? {
                Some(scale) => f64::from(scale),
                None => return Ok(Plain::Na),
            };
            let (Some(weight), Some(height)) = (double(&values[0])?, double(&values[1])?) else {
                return Ok(Plain::Na);
            };
            let ratio = height / scale;
            Ok(Plain::Double(weight / (ratio * ratio)))
        },
    },
    Function {
        name: "project_ratio",
        formals: &[
            ("numerator", None),
            ("denominator", None),
            ("decimals", Some(|| Plain::Double(2.0))),
            ("adjust", Some(|| Plain::Double(0.0))),
            ("as_percent", Some(|| Plain::Logical(false))),
        ],
        body: |values| {
            let (Some(numerator), Some(denominator)) = (double(&values[0])?, double(&values[1])?)
            else {
                return Ok(Plain::Na);
            };
            let mut value = numerator / denominator;
            if condition(&values[4])? {
                value *= 100.0;
            }
            let decimals =
                as_integer(&values[2])?.ok_or("missing value where TRUE/FALSE needed")?;
            let value = project_round(value, decimals)?;
            // is.na(adjust) counts as zero.
            let adjust = double(&values[3])?.unwrap_or(0.0);
            Ok(Plain::Double(value + adjust))
        },
    },
    Function {
        name: "numeric_constant",
        formals: &[("kind", None)],
        body: |values| {
            let kind = match &values[0] {
                Plain::Character(kind) => kind.as_str(),
                Plain::Double(_) | Plain::Logical(_) | Plain::Na => {
                    return Err("subscript out of bounds".to_owned())
                }
            };
            Ok(Plain::Double(match kind {
                "zero" => 0.0,
                "one" => 1.0,
                "positive-infinity" => f64::INFINITY,
                "negative-infinity" => f64::NEG_INFINITY,
                "nan" => f64::NAN,
                _ => return Err("subscript out of bounds".to_owned()),
            }))
        },
    },
    Function {
        name: "normal_cdf",
        formals: &[("q", None)],
        body: |values| {
            Ok(match double(&values[0])? {
                Some(q) => Plain::Double(normal_cdf(q)),
                None => Plain::Na,
            })
        },
    },
    Function {
        name: "project_value",
        formals: &[("x", None)],
        body: |values| Ok(values[0].clone()),
    },
    Function {
        name: "dose_per_kg",
        formals: &[("dose_mg", None), ("weight_kg", None)],
        body: |values| {
            let weight = double(&values[1])?
                .filter(|value| !value.is_nan())
                .ok_or("missing value where TRUE/FALSE needed")?;
            if weight <= 0.0 {
                return Err("weight_kg must be positive".to_owned());
            }
            Ok(match double(&values[0])? {
                Some(dose) => Plain::Double(dose / weight),
                None => Plain::Na,
            })
        },
    },
];

/// A value R reads as a double: `NA` is `None`, a logical is 0 or 1.
fn double(value: &Plain) -> Result<Option<f64>, String> {
    match value {
        Plain::Na => Ok(None),
        Plain::Double(value) => Ok(Some(*value)),
        Plain::Logical(value) => Ok(Some(f64::from(u8::from(*value)))),
        Plain::Character(_) => Err("non-numeric argument to binary operator".to_owned()),
    }
}

/// R's `as.integer` of a double: truncation toward zero, and `NA` outside
/// the integer range or for `NA` and NaN.
fn as_integer(value: &Plain) -> Result<Option<i32>, String> {
    Ok(double(value)?.and_then(|value| {
        (value > f64::from(i32::MIN) && value < f64::from(i32::MAX) + 1.0)
            .then(|| value.trunc() as i32)
    }))
}

/// The value of an `if` condition, as R reads a length-one vector.
fn condition(value: &Plain) -> Result<bool, String> {
    match double(value)? {
        Some(value) if !value.is_nan() => Ok(value != 0.0),
        _ => Err("missing value where TRUE/FALSE needed".to_owned()),
    }
}

/// `.project_round`: round the exact stored binary64 value to `decimals`
/// places with ties to even, as Python's `round()` does, working on its
/// exact decimal digits.
fn project_round(value: f64, decimals: i32) -> Result<f64, String> {
    if !value.is_finite() || value == 0.0 || decimals > 323 {
        return Ok(value);
    }
    if decimals < -308 {
        return Ok(value * 0.0);
    }
    // Little-endian decimal digits keep every operation small and exact.
    fn multiply(digits: &mut Vec<u8>, factor: u8, mut carry: u8) {
        for digit in digits.iter_mut() {
            let product = *digit * factor + carry;
            *digit = product % 10;
            carry = product / 10;
        }
        while carry > 0 {
            digits.push(carry % 10);
            carry /= 10;
        }
    }
    let bits = value.abs().to_bits();
    let exponent = i32::try_from(bits >> 52).expect("the sign bit is clear");
    let mut digits = vec![u8::from(exponent != 0)];
    for bit in (0..52).rev() {
        multiply(&mut digits, 2, u8::from((bits >> bit) & 1 == 1));
    }
    let shift = exponent.max(1) - 1023 - 52;
    let scale = (-shift).max(0);
    for _ in 0..shift.abs() {
        multiply(&mut digits, if shift < 0 { 5 } else { 2 }, 0);
    }
    let discard = scale - decimals;
    if discard <= 0 {
        return Ok(value);
    }
    let discard = usize::try_from(discard).expect("positive");
    if digits.len() <= discard {
        digits.resize(discard + 1, 0);
    }
    let mut kept = digits.split_off(discard);
    let remainder = digits;
    let halfway = remainder[discard - 1];
    if halfway > 5
        || (halfway == 5
            && (remainder[..discard - 1].iter().any(|digit| *digit != 0) || kept[0] % 2 == 1))
    {
        multiply(&mut kept, 1, 1);
    }
    let text = format!(
        "{}{}e{}",
        if value < 0.0 { "-" } else { "" },
        kept.iter()
            .rev()
            .map(|digit| char::from(b'0' + digit))
            .collect::<String>(),
        -decimals
    );
    let rounded: f64 = text.parse().expect("a decimal numeral parses");
    if rounded.is_finite() {
        Ok(rounded)
    } else {
        Err("rounded value is too large".to_owned())
    }
}

// The R and Python sources spell their constants as decimal literals. `LN_2`,
// `SQRT_2` and `FRAC_2_SQRT_PI` from the standard library are the binary64
// values those literals read as; the two below have no standard constant.
const ONE_OVER_SQRT_PI: f64 = 0.5641895835477563;
const TINY: f64 = 1e-300;

/// `.project_exp`: e to the power `x`, by the authored series.
fn exp(x: f64) -> f64 {
    let doubling = (x / LN_2 + if x >= 0.0 { 0.5 } else { -0.5 }).trunc();
    let remainder = x - doubling * LN_2;
    let (mut term, mut total, mut step) = (1.0_f64, 1.0_f64, 0.0);
    loop {
        step += 1.0;
        term *= remainder / step;
        total += term;
        if term.abs() <= 1e-18 * total.abs() {
            break;
        }
    }
    total * 2.0_f64.powi(doubling as i32)
}

/// `.erf_series`: the error function's Maclaurin series, for a small
/// argument.
fn erf_series(z: f64) -> f64 {
    let square = z * z;
    let (mut power, mut total, mut step) = (z, z, 0.0);
    loop {
        step += 1.0;
        power *= -square / step;
        let contribution = power / (2.0 * step + 1.0);
        total += contribution;
        if contribution.abs() <= 1e-18 * total.abs() {
            break;
        }
    }
    FRAC_2_SQRT_PI * total
}

/// `.erfc_tail`: the complementary error function of a positive argument,
/// by its continued fraction.
fn erfc_tail(z: f64) -> f64 {
    let mut fraction = TINY;
    let mut numerator = fraction;
    let mut denominator = 0.0;
    let mut step = 1.0;
    while step < 300.0 {
        let partial = if step == 1.0 { 1.0 } else { (step - 1.0) / 2.0 };
        denominator = z + partial * denominator;
        if denominator == 0.0 {
            denominator = TINY;
        }
        numerator = z + partial / numerator;
        if numerator == 0.0 {
            numerator = TINY;
        }
        denominator = 1.0 / denominator;
        let factor = numerator * denominator;
        fraction *= factor;
        if (factor - 1.0).abs() < 1e-17 {
            break;
        }
        step += 1.0;
    }
    ONE_OVER_SQRT_PI * exp(-z * z) * fraction
}

fn erfc(z: f64) -> f64 {
    if z < -2.0 {
        return 2.0 - erfc_tail(-z);
    }
    if z > 2.0 {
        return erfc_tail(z);
    }
    1.0 - erf_series(z)
}

/// `normal_cdf`: the probability that a standard normal deviate is at most
/// `q`.
fn normal_cdf(q: f64) -> f64 {
    0.5 * erfc(-q / SQRT_2)
}

/// The package's functions behind the engine's host-functions port, claiming
/// R. A bound handle holds the function and its declared result type.
pub struct Benchmarks;

/// An R error raised in a call, as the R host classifies it.
fn raised(message: impl Into<String>) -> FunctionError {
    FunctionError::Exception {
        class: "simpleError".to_owned(),
        message: message.into(),
        payload: None,
    }
}

/// One engine argument as the plain R value the R binding passes.
fn plain(value: ScalarRef<'_>) -> Result<Plain, String> {
    /// The largest magnitude up to which binary64 holds every integer.
    const EXACT: i64 = 1 << 53;
    Ok(match value {
        ScalarRef::Missing => Plain::Na,
        ScalarRef::Str(text) => Plain::Character(text.to_owned()),
        ScalarRef::Int(value) if (-EXACT..=EXACT).contains(&value) => Plain::Double(value as f64),
        ScalarRef::Int(value) => {
            return Err(format!(
                "the integer {value} cannot be held exactly by an R double"
            ))
        }
        ScalarRef::Float(value) => Plain::Double(value),
        ScalarRef::Bool(value) => Plain::Logical(value),
        _ => return Err("this port takes no temporal arguments".to_owned()),
    })
}

/// A returned plain value converted by the declared result type, as the R
/// binding converts it: `NA` and a non-finite double are missing.
fn result(value: Plain, returns: ColumnType) -> Result<Scalar, String> {
    match (value, returns) {
        (Plain::Na, _) => Ok(Scalar::Missing),
        (Plain::Double(value), _) if !value.is_finite() => Ok(Scalar::Missing),
        (Plain::Double(value), ColumnType::Float) => Ok(Scalar::float(value)),
        (Plain::Double(value), ColumnType::Int)
            if value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0 =>
        {
            Ok(Scalar::Int(value as i64))
        }
        (Plain::Character(text), ColumnType::Str) => Ok(Scalar::Str(text)),
        (value, returns) => Err(format!("a {returns:?} result cannot be {value:?}")),
    }
}

impl ProjectFunctions for Benchmarks {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        if lock.language != HostLanguage::R || lock.kind != LockKind::Renv {
            return Err(FunctionError::Protocol);
        }
        let locked = yamaa::decode_renv_lock(lock.bytes).map_err(FunctionError::LockSyntax)?;
        if functions.is_empty() {
            return Ok(());
        }
        let mut packages = functions
            .iter()
            .map(|function| function.call.split("::").next().unwrap_or_default())
            .collect::<BTreeSet<_>>();
        packages.insert("yamaa");
        let mut findings = Vec::new();
        for package in packages {
            let Some(expected) = locked.packages.get(package) else {
                findings.push(LockFinding::new(
                    package.to_owned(),
                    LockReason::VersionNotLocked,
                    Vec::new(),
                    None,
                ));
                continue;
            };
            let installed = match package {
                "yamaa" => yamaa::VERSION,
                PACKAGE => PACKAGE_VERSION,
                _ => {
                    findings.push(LockFinding::new(
                        package.to_owned(),
                        LockReason::PackageNotInstalled,
                        vec![expected.clone()],
                        None,
                    ));
                    continue;
                }
            };
            if installed != expected {
                findings.push(LockFinding::new(
                    package.to_owned(),
                    LockReason::VersionMismatch,
                    vec![expected.clone()],
                    Some(installed.to_owned()),
                ));
            }
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(FunctionError::Lock(findings))
        }
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        let Some((package, name)) = identity
            .call
            .split_once("::")
            .filter(|(package, name)| !package.is_empty() && !name.is_empty())
        else {
            return Err(raised("callable must be package-qualified"));
        };
        if package != PACKAGE {
            return Err(raised(format!("there is no package called '{package}'")));
        }
        let function = FUNCTIONS
            .into_iter()
            .find(|function| function.name == name)
            .ok_or_else(|| {
                raised(format!(
                    "'{name}' is not an exported object from 'namespace:{package}'"
                ))
            })?;
        let declared = signature
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>();
        let unique = declared.iter().collect::<BTreeSet<_>>();
        if unique.len() != declared.len()
            || function.formals().collect::<BTreeSet<_>>() != declared.iter().copied().collect()
        {
            return Err(raised("installed callable signature differs from params"));
        }
        Ok(FunctionHandle::new((function, signature.returns)))
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        let (function, returns) = handle
            .with::<(Function, ColumnType), _>(|bound| *bound)
            .ok_or(CallError::Raised(FunctionError::Protocol))?;
        let arguments = arguments
            .iter()
            .map(|argument| {
                plain(argument.value)
                    .map(|value| (argument.name, value))
                    .map_err(|reason| {
                        CallError::Raised(FunctionError::Representation {
                            message: format!("argument {}: {reason}", argument.name),
                            payload: None,
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = function
            .call(&arguments)
            .map_err(|message| CallError::Raised(raised(message)))?;
        result(value, returns).map_err(|message| {
            CallError::InvalidResult(FunctionError::Representation {
                message,
                payload: None,
            })
        })
    }
}

/// [`Benchmarks`] with one function that strays from the package: it returns
/// one more than the package's result, so it breaks the function's declared
/// contract tests.
pub struct Strayed(pub &'static str);

impl ProjectFunctions for Strayed {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Benchmarks.verify_lock(lock, functions)
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        Benchmarks.bind(identity, signature)
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        let strays = handle
            .with::<(Function, ColumnType), _>(|(function, _)| function.name == self.0)
            .unwrap_or(false);
        match Benchmarks.invoke(handle, arguments)? {
            Scalar::Float(value) if strays => Ok(Scalar::float(value + 1.0)),
            value => Ok(value),
        }
    }
}

/// The authored parity checks of `rust/tests/project-programs/r/tests/
/// numeric-parity.R`, through the port.
#[test]
fn the_port_returns_the_packages_values() {
    let call = |name: &str, arguments: &[(&str, Plain)]| {
        FUNCTIONS
            .into_iter()
            .find(|function| function.name == name)
            .unwrap()
            .call(arguments)
            .unwrap()
    };
    let double = |value: f64| Plain::Double(value);
    let ratio = |numerator: f64, decimals: f64| {
        call(
            "project_ratio",
            &[
                ("numerator", double(numerator)),
                ("denominator", double(1.0)),
                ("decimals", double(decimals)),
                ("adjust", double(0.0)),
                ("as_percent", Plain::Logical(false)),
            ],
        )
    };
    assert_eq!(
        call("normal_cdf", &[("q", double(-3.0))]),
        double(f64::from_bits(0x3f561de1f985b5dc))
    );
    assert_eq!(ratio(2.0 / 400.0, 2.0), double(0.01));
    assert_eq!(ratio(-2.0 / 400.0, 2.0), double(-0.01));
    assert_eq!(ratio(25.0 / 100.0, 1.0), double(0.2));
    assert_eq!(ratio(5.0 / 4.0, 1.0), double(1.2));
    assert_eq!(ratio(1234.0, -2.0), double(1200.0));
    for sign in [-1.0, 1.0] {
        assert_eq!(
            ratio(sign * f64::from_bits(0x3f747ae147ae147a), 2.0),
            double(sign * 0.0)
        );
        assert_eq!(
            ratio(sign * f64::from_bits(0x3f747ae147ae147b), 2.0),
            double(sign * 0.01)
        );
        assert_eq!(ratio(sign * 2.675, 2.0), double(sign * 2.67));
        assert_eq!(ratio(sign * 1.75, 1.0), double(sign * 1.8));
        assert_eq!(ratio(sign * 50.0, -2.0), double(sign * 0.0));
        assert_eq!(ratio(sign * 150.0, -2.0), double(sign * 200.0));
    }
    let subnormal = f64::from_bits(1);
    assert_eq!(ratio(subnormal, 323.0), double(0.0));
    assert_eq!(ratio(subnormal, 324.0), double(subnormal));
    assert_eq!(ratio(1e300, -309.0), double(0.0));
    assert_eq!(
        call(
            "project_ratio",
            &[
                ("numerator", double(2.0)),
                ("denominator", double(400.0)),
                ("decimals", double(2.0)),
                ("adjust", Plain::Na),
                ("as_percent", Plain::Logical(true)),
            ],
        ),
        double(0.5)
    );
    assert_eq!(
        call(
            "bmi",
            &[("weight_kg", double(64.0)), ("height_cm", double(160.0))]
        ),
        double(24.999999999999996)
    );
}
