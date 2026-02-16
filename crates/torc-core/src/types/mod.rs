//! The Torc type universe.
//!
//! Unifies dependent types, linear types, effect types, and resource types
//! into a single coherent system. Every computation node carries a full
//! type signature that encodes correctness properties, resource ownership,
//! effects, and resource bounds.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

pub mod check;

/// Signedness of an integer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Signedness {
    Signed,
    Unsigned,
}

/// IEEE 754 floating-point precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FloatPrecision {
    F16,
    F32,
    F64,
    F128,
}

/// Linearity annotation controlling ownership semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Linearity {
    /// Must be used exactly once (consumed).
    Linear,
    /// May be used at most once (consumed or dropped).
    Affine,
    /// May be aliased, immutable access only.
    Shared,
    /// Single owner, mutable access, transferable.
    Unique,
    /// Reference-counted shared ownership.
    Counted,
    /// No linearity constraint (default for primitives).
    Unrestricted,
}

/// Effect kinds that a computation may perform.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Effect {
    /// No side effects; result depends only on inputs.
    Pure,
    /// Allocates memory in the named region.
    Alloc(String),
    /// Performs I/O on the named device/descriptor.
    IO(String),
    /// Atomic operation with the specified ordering.
    Atomic(String),
    /// Calls foreign code with the specified ABI.
    FFI(String),
    /// May not terminate (must carry justification).
    Diverge,
    /// May abort execution (must carry recovery strategy).
    Panic,
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Pure => write!(f, "Pure"),
            Effect::Alloc(r) => write!(f, "Alloc<{r}>"),
            Effect::IO(d) => write!(f, "IO<{d}>"),
            Effect::Atomic(o) => write!(f, "Atomic<{o}>"),
            Effect::FFI(abi) => write!(f, "FFI<{abi}>"),
            Effect::Diverge => write!(f, "Diverge"),
            Effect::Panic => write!(f, "Panic"),
        }
    }
}

/// A predicate expression for refinement types and contracts.
///
/// Represents first-order logic with arithmetic, used in `where` clauses
/// and contract pre/postconditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Predicate {
    /// Boolean literal.
    BoolLit(bool),
    /// Integer literal.
    IntLit(i128),
    /// Float literal.
    FloatLit(f64),
    /// Reference to a named variable (e.g., "value", "output", "len").
    Var(String),

    // Arithmetic
    /// Addition.
    Add(Box<Predicate>, Box<Predicate>),
    /// Subtraction.
    Sub(Box<Predicate>, Box<Predicate>),
    /// Multiplication.
    Mul(Box<Predicate>, Box<Predicate>),
    /// Division.
    Div(Box<Predicate>, Box<Predicate>),
    /// Modulo.
    Mod(Box<Predicate>, Box<Predicate>),
    /// Negation.
    Neg(Box<Predicate>),

    // Comparison
    /// Equal.
    Eq(Box<Predicate>, Box<Predicate>),
    /// Not equal.
    Ne(Box<Predicate>, Box<Predicate>),
    /// Less than.
    Lt(Box<Predicate>, Box<Predicate>),
    /// Less than or equal.
    Le(Box<Predicate>, Box<Predicate>),
    /// Greater than.
    Gt(Box<Predicate>, Box<Predicate>),
    /// Greater than or equal.
    Ge(Box<Predicate>, Box<Predicate>),

    // Logical
    /// Logical AND.
    And(Box<Predicate>, Box<Predicate>),
    /// Logical OR.
    Or(Box<Predicate>, Box<Predicate>),
    /// Logical NOT.
    Not(Box<Predicate>),
    /// Logical implication (a => b).
    Implies(Box<Predicate>, Box<Predicate>),

    // Quantifiers
    /// Universal quantification: forall x in range, predicate holds.
    ForAll {
        var: String,
        range: Box<Predicate>,
        body: Box<Predicate>,
    },
    /// Existential quantification: exists x in range such that predicate holds.
    Exists {
        var: String,
        range: Box<Predicate>,
        body: Box<Predicate>,
    },

    // Function application (for reference functions like sorted(), len(), etc.)
    /// Named function application.
    Apply(String, Vec<Predicate>),
}

impl Predicate {
    /// Convenience: create `value >= lo && value <= hi`.
    pub fn in_range(var: &str, lo: i128, hi: i128) -> Self {
        Predicate::And(
            Box::new(Predicate::Ge(
                Box::new(Predicate::Var(var.to_string())),
                Box::new(Predicate::IntLit(lo)),
            )),
            Box::new(Predicate::Le(
                Box::new(Predicate::Var(var.to_string())),
                Box::new(Predicate::IntLit(hi)),
            )),
        )
    }

    /// Combine two predicates with logical AND.
    /// Simplifies trivially: conjoin with True returns the other.
    pub fn conjoin(self, other: Predicate) -> Predicate {
        match (&self, &other) {
            (Predicate::BoolLit(true), _) => other,
            (_, Predicate::BoolLit(true)) => self,
            _ => Predicate::And(Box::new(self), Box::new(other)),
        }
    }

    /// Convenience: `value > 0`.
    pub fn positive(var: &str) -> Self {
        Predicate::Gt(
            Box::new(Predicate::Var(var.to_string())),
            Box::new(Predicate::IntLit(0)),
        )
    }
}

/// The core type representation.
///
/// Every value in a Torc graph has a `Type` that encodes its structure,
/// constraints, ownership, and effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    // === Primitives ===
    /// The empty type (no values).
    Void,
    /// The singleton type (exactly one value).
    Unit,
    /// Boolean: {true, false}.
    Bool,
    /// Integer with explicit width (1..128 bits) and signedness.
    Int { width: u8, signedness: Signedness },
    /// IEEE 754 floating-point with explicit precision.
    Float { precision: FloatPrecision },
    /// Fixed-point with total bits and fractional bits.
    Fixed { total_bits: u8, frac_bits: u8 },

    // === Composite ===
    /// Heterogeneous fixed-length product.
    Tuple(Vec<Type>),
    /// Named-field product type.
    Record(BTreeMap<String, Type>),
    /// Tagged union (sum type). All cases must be covered in Switch nodes.
    Variant(BTreeMap<String, Type>),
    /// Fixed-length homogeneous sequence.
    Array { element: Box<Type>, length: usize },
    /// Variable-length homogeneous sequence with capacity tracking.
    Vec { element: Box<Type> },

    // === Refinement ===
    /// A type refined by a predicate: `T where P`.
    Refined {
        base: Box<Type>,
        predicate: Predicate,
    },

    // === Linearity ===
    /// A type with a linearity annotation controlling ownership.
    Linear {
        inner: Box<Type>,
        linearity: Linearity,
    },

    // === Resource ===
    /// Value produced within a time bound.
    Timed { inner: Box<Type>, bound: TimeBound },
    /// Value occupying at most the given number of bytes.
    Sized { inner: Box<Type>, max_bytes: usize },
    /// Value produced within an energy budget (microjoules).
    Powered { inner: Box<Type>, energy_uj: u64 },
    /// Value transmitted within a throughput bound.
    Bandwidth { inner: Box<Type>, min_bps: u64 },

    // === Probability ===
    /// A probability distribution over a type.
    Distribution(Box<Type>),
    /// Distribution conditioned on evidence.
    Posterior { inner: Box<Type>, evidence: String },
    /// Confidence interval at a given confidence level (0.0..1.0).
    Interval { inner: Box<Type>, confidence: f64 },
    /// Value with bounded approximation error.
    Approximate { inner: Box<Type>, max_error: f64 },

    // === Dependent ===
    /// A named type with value parameters (e.g., Matrix<T, Rows, Cols>).
    Parameterized {
        name: String,
        type_params: Vec<Type>,
        value_params: Vec<ValueParam>,
    },

    // === Special ===
    /// Optional value (nullable).
    Option(Box<Type>),
    /// A named type reference (resolved during linking).
    Named(String),
}

/// A value parameter for dependent types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValueParam {
    /// A concrete integer value.
    Concrete(i128),
    /// A symbolic name to be resolved.
    Symbolic(String),
}

/// Time bound specification for resource types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeBound {
    /// Worst-case execution time in nanoseconds.
    pub wcet_ns: u64,
    /// Target description (e.g., "arm-cortex-m4f-168mhz").
    pub target: String,
}

/// The type signature of a computation node: its input and output types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeSignature {
    /// Types of input ports, in order.
    pub inputs: Vec<Type>,
    /// Types of output ports, in order.
    pub outputs: Vec<Type>,
}

impl TypeSignature {
    pub fn new(inputs: Vec<Type>, outputs: Vec<Type>) -> Self {
        Self { inputs, outputs }
    }

    /// Pure function: single output, no effects implied by signature.
    pub fn pure_fn(inputs: Vec<Type>, output: Type) -> Self {
        Self {
            inputs,
            outputs: vec![output],
        }
    }

    /// A node that takes no inputs and produces a single output (e.g., Literal).
    pub fn source(output: Type) -> Self {
        Self {
            inputs: vec![],
            outputs: vec![output],
        }
    }

    /// A node that takes a single input and produces no outputs (e.g., sink).
    pub fn sink(input: Type) -> Self {
        Self {
            inputs: vec![input],
            outputs: vec![],
        }
    }
}

// === Convenience constructors ===

impl Type {
    pub fn i8() -> Self {
        Type::Int {
            width: 8,
            signedness: Signedness::Signed,
        }
    }
    pub fn i16() -> Self {
        Type::Int {
            width: 16,
            signedness: Signedness::Signed,
        }
    }
    pub fn i32() -> Self {
        Type::Int {
            width: 32,
            signedness: Signedness::Signed,
        }
    }
    pub fn i64() -> Self {
        Type::Int {
            width: 64,
            signedness: Signedness::Signed,
        }
    }
    pub fn u8() -> Self {
        Type::Int {
            width: 8,
            signedness: Signedness::Unsigned,
        }
    }
    pub fn u16() -> Self {
        Type::Int {
            width: 16,
            signedness: Signedness::Unsigned,
        }
    }
    pub fn u32() -> Self {
        Type::Int {
            width: 32,
            signedness: Signedness::Unsigned,
        }
    }
    pub fn u64() -> Self {
        Type::Int {
            width: 64,
            signedness: Signedness::Unsigned,
        }
    }
    pub fn f32() -> Self {
        Type::Float {
            precision: FloatPrecision::F32,
        }
    }
    pub fn f64() -> Self {
        Type::Float {
            precision: FloatPrecision::F64,
        }
    }

    /// Wrap this type in a refinement predicate.
    pub fn refined(self, predicate: Predicate) -> Self {
        Type::Refined {
            base: Box::new(self),
            predicate,
        }
    }

    /// Wrap this type with a linearity annotation.
    pub fn with_linearity(self, linearity: Linearity) -> Self {
        Type::Linear {
            inner: Box::new(self),
            linearity,
        }
    }

    /// Wrap this type as linear (must be consumed exactly once).
    pub fn linear(self) -> Self {
        self.with_linearity(Linearity::Linear)
    }

    /// Wrap this type as shared (immutable aliased access).
    pub fn shared(self) -> Self {
        self.with_linearity(Linearity::Shared)
    }

    /// Wrap with a time bound.
    pub fn timed(self, wcet_ns: u64, target: &str) -> Self {
        Type::Timed {
            inner: Box::new(self),
            bound: TimeBound {
                wcet_ns,
                target: target.to_string(),
            },
        }
    }

    /// Wrap with a size bound.
    pub fn sized(self, max_bytes: usize) -> Self {
        Type::Sized {
            inner: Box::new(self),
            max_bytes,
        }
    }

    /// Wrap with a minimum bandwidth bound (bytes per second).
    pub fn bandwidth(self, min_bps: u64) -> Self {
        Type::Bandwidth {
            inner: Box::new(self),
            min_bps,
        }
    }

    /// Wrap with an energy budget (microjoules).
    pub fn powered(self, energy_uj: u64) -> Self {
        Type::Powered {
            inner: Box::new(self),
            energy_uj,
        }
    }

    /// Extract the innermost linearity annotation, if any.
    /// Peels through Timed, Sized, Powered, Bandwidth, Refined wrappers.
    pub fn linearity(&self) -> Option<Linearity> {
        match self {
            Type::Linear { linearity, .. } => Some(*linearity),
            Type::Timed { inner, .. }
            | Type::Sized { inner, .. }
            | Type::Powered { inner, .. }
            | Type::Bandwidth { inner, .. }
            | Type::Refined { base: inner, .. } => inner.linearity(),
            _ => None,
        }
    }

    /// Extract the innermost base type, peeling all wrappers.
    pub fn base_type(&self) -> &Type {
        match self {
            Type::Linear { inner, .. }
            | Type::Timed { inner, .. }
            | Type::Sized { inner, .. }
            | Type::Powered { inner, .. }
            | Type::Bandwidth { inner, .. }
            | Type::Refined { base: inner, .. } => inner.base_type(),
            _ => self,
        }
    }

    /// Check if this type is a primitive (non-composite, non-wrapped).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Void
                | Type::Unit
                | Type::Bool
                | Type::Int { .. }
                | Type::Float { .. }
                | Type::Fixed { .. }
        )
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "Void"),
            Type::Unit => write!(f, "Unit"),
            Type::Bool => write!(f, "Bool"),
            Type::Int { width, signedness } => {
                let prefix = match signedness {
                    Signedness::Signed => "i",
                    Signedness::Unsigned => "u",
                };
                write!(f, "{prefix}{width}")
            }
            Type::Float { precision } => match precision {
                FloatPrecision::F16 => write!(f, "f16"),
                FloatPrecision::F32 => write!(f, "f32"),
                FloatPrecision::F64 => write!(f, "f64"),
                FloatPrecision::F128 => write!(f, "f128"),
            },
            Type::Fixed {
                total_bits,
                frac_bits,
            } => write!(f, "Fixed<{total_bits}, {frac_bits}>"),
            Type::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{e}")?;
                }
                write!(f, ")")
            }
            Type::Record(fields) => {
                write!(f, "{{")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {ty}")?;
                }
                write!(f, "}}")
            }
            Type::Variant(cases) => {
                write!(f, "Variant<")?;
                for (i, (tag, ty)) in cases.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{tag}({ty})")?;
                }
                write!(f, ">")
            }
            Type::Array { element, length } => write!(f, "[{element}; {length}]"),
            Type::Vec { element } => write!(f, "Vec<{element}>"),
            Type::Refined { base, .. } => write!(f, "{base} where <predicate>"),
            Type::Linear { inner, linearity } => write!(f, "{linearity:?}<{inner}>"),
            Type::Timed { inner, bound } => {
                write!(f, "Timed<{inner}, {}ns @ {}>", bound.wcet_ns, bound.target)
            }
            Type::Sized { inner, max_bytes } => write!(f, "Sized<{inner}, {max_bytes}B>"),
            Type::Powered { inner, energy_uj } => write!(f, "Powered<{inner}, {energy_uj}μJ>"),
            Type::Bandwidth { inner, min_bps } => write!(f, "Bandwidth<{inner}, {min_bps}Bps>"),
            Type::Distribution(inner) => write!(f, "Distribution<{inner}>"),
            Type::Posterior { inner, evidence } => {
                write!(f, "Posterior<{inner}, {evidence}>")
            }
            Type::Interval { inner, confidence } => {
                write!(f, "Interval<{inner}, {confidence}>")
            }
            Type::Approximate { inner, max_error } => {
                write!(f, "Approximate<{inner}, {max_error}>")
            }
            Type::Parameterized {
                name,
                type_params,
                value_params,
            } => {
                write!(f, "{name}<")?;
                for (i, tp) in type_params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{tp}")?;
                }
                for vp in value_params {
                    write!(f, ", ")?;
                    match vp {
                        ValueParam::Concrete(v) => write!(f, "{v}")?,
                        ValueParam::Symbolic(s) => write!(f, "{s}")?,
                    }
                }
                write!(f, ">")
            }
            Type::Option(inner) => write!(f, "Option<{inner}>"),
            Type::Named(name) => write!(f, "{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_constructors() {
        assert!(Type::i32().is_primitive());
        assert!(Type::f64().is_primitive());
        assert!(Type::Bool.is_primitive());
        assert!(Type::Void.is_primitive());
        assert!(Type::Unit.is_primitive());
    }

    #[test]
    fn refinement_type() {
        let positive_int = Type::i32().refined(Predicate::positive("value"));
        assert!(!positive_int.is_primitive());
        match &positive_int {
            Type::Refined { base, predicate } => {
                assert_eq!(**base, Type::i32());
                assert!(matches!(predicate, Predicate::Gt(..)));
            }
            _ => panic!("expected Refined"),
        }
    }

    #[test]
    fn linear_type() {
        let linear_handle = Type::u64().linear();
        match &linear_handle {
            Type::Linear { inner, linearity } => {
                assert_eq!(**inner, Type::u64());
                assert_eq!(*linearity, Linearity::Linear);
            }
            _ => panic!("expected Linear"),
        }
    }

    #[test]
    fn composite_types() {
        let tuple = Type::Tuple(vec![Type::i32(), Type::f32()]);
        assert!(!tuple.is_primitive());
        assert_eq!(format!("{tuple}"), "(i32, f32)");

        let array = Type::Array {
            element: Box::new(Type::u8()),
            length: 256,
        };
        assert_eq!(format!("{array}"), "[u8; 256]");
    }

    #[test]
    fn type_signature() {
        let sig = TypeSignature::pure_fn(vec![Type::f32(), Type::f32()], Type::f32());
        assert_eq!(sig.inputs.len(), 2);
        assert_eq!(sig.outputs.len(), 1);
    }

    #[test]
    fn resource_types() {
        let timed = Type::f32().timed(50_000, "arm-cortex-m4f-168mhz");
        match &timed {
            Type::Timed { bound, .. } => {
                assert_eq!(bound.wcet_ns, 50_000);
                assert_eq!(bound.target, "arm-cortex-m4f-168mhz");
            }
            _ => panic!("expected Timed"),
        }

        let sized = Type::f32().sized(4);
        match &sized {
            Type::Sized { max_bytes, .. } => assert_eq!(*max_bytes, 4),
            _ => panic!("expected Sized"),
        }
    }

    #[test]
    fn dependent_type() {
        // Matrix<f32, 3, 4>
        let matrix = Type::Parameterized {
            name: "Matrix".to_string(),
            type_params: vec![Type::f32()],
            value_params: vec![ValueParam::Concrete(3), ValueParam::Concrete(4)],
        };
        assert_eq!(format!("{matrix}"), "Matrix<f32, 3, 4>");
    }

    #[test]
    fn predicate_in_range() {
        let pred = Predicate::in_range("value", 0, 4095);
        match &pred {
            Predicate::And(lhs, rhs) => {
                assert!(matches!(lhs.as_ref(), Predicate::Ge(..)));
                assert!(matches!(rhs.as_ref(), Predicate::Le(..)));
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn bandwidth_type() {
        let bw = Type::u8().bandwidth(1_000_000);
        match &bw {
            Type::Bandwidth { inner, min_bps } => {
                assert_eq!(**inner, Type::u8());
                assert_eq!(*min_bps, 1_000_000);
            }
            _ => panic!("expected Bandwidth"),
        }
        assert_eq!(format!("{bw}"), "Bandwidth<u8, 1000000Bps>");
    }

    #[test]
    fn powered_type() {
        let pw = Type::f32().powered(500);
        match &pw {
            Type::Powered { inner, energy_uj } => {
                assert_eq!(**inner, Type::f32());
                assert_eq!(*energy_uj, 500);
            }
            _ => panic!("expected Powered"),
        }
    }

    #[test]
    fn linearity_helper_through_wrappers() {
        // Timed<Sized<Linear<i32>>> should extract Linear
        let wrapped = Type::i32()
            .with_linearity(Linearity::Linear)
            .sized(4)
            .timed(100, "test");
        assert_eq!(wrapped.linearity(), Some(Linearity::Linear));

        // Bandwidth<Powered<Affine<f32>>> should extract Affine
        let wrapped2 = Type::f32()
            .with_linearity(Linearity::Affine)
            .powered(100)
            .bandwidth(1000);
        assert_eq!(wrapped2.linearity(), Some(Linearity::Affine));

        // Refined<Unique<i32>> should extract Unique
        let wrapped3 = Type::i32()
            .with_linearity(Linearity::Unique)
            .refined(Predicate::positive("value"));
        assert_eq!(wrapped3.linearity(), Some(Linearity::Unique));

        // Bare i32 has no linearity
        assert_eq!(Type::i32().linearity(), None);
    }

    #[test]
    fn base_type_peeling() {
        // Timed<Sized<Linear<Refined<f32>>>> should peel to f32
        let nested = Type::f32()
            .refined(Predicate::positive("value"))
            .with_linearity(Linearity::Linear)
            .sized(4)
            .timed(100, "test");
        assert_eq!(*nested.base_type(), Type::f32());

        // Bare type returns itself
        assert_eq!(*Type::i32().base_type(), Type::i32());

        // Bandwidth<Powered<u8>> peels to u8
        let bw = Type::u8().powered(50).bandwidth(1000);
        assert_eq!(*bw.base_type(), Type::u8());
    }

    #[test]
    fn predicate_conjoin_simplifies() {
        let trivial = Predicate::BoolLit(true);
        let pred = Predicate::positive("x");

        // true AND p => p
        let result = trivial.clone().conjoin(pred.clone());
        assert_eq!(result, pred);

        // p AND true => p
        let result2 = pred.clone().conjoin(Predicate::BoolLit(true));
        assert_eq!(result2, pred);
    }

    #[test]
    fn predicate_conjoin_both_nontrivial() {
        let a = Predicate::positive("x");
        let b = Predicate::positive("y");
        let result = a.clone().conjoin(b.clone());
        assert!(matches!(result, Predicate::And(..)));
        if let Predicate::And(lhs, rhs) = result {
            assert_eq!(*lhs, a);
            assert_eq!(*rhs, b);
        }
    }

    #[test]
    fn sensor_voltage_type() {
        // From spec: Timed<Sized<Linear<Float<32> where value >= 0.0 && value <= 5.0>, 4>, 50μs>
        let sensor_type = Type::f32()
            .refined(Predicate::And(
                Box::new(Predicate::Ge(
                    Box::new(Predicate::Var("value".into())),
                    Box::new(Predicate::FloatLit(0.0)),
                )),
                Box::new(Predicate::Le(
                    Box::new(Predicate::Var("value".into())),
                    Box::new(Predicate::FloatLit(5.0)),
                )),
            ))
            .linear()
            .sized(4)
            .timed(50_000, "arm-cortex-m4f-168mhz");

        // Should nest as: Timed<Sized<Linear<Refined<f32>>>>
        assert!(matches!(sensor_type, Type::Timed { .. }));
    }
}
