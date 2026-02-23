//! Torc Type to LLVM type mapping.

use inkwell::context::Context;
use inkwell::types::{BasicType, BasicTypeEnum};
use torc_core::types::{FloatPrecision, Type};

/// Convert a Torc `Type` to an LLVM `BasicTypeEnum`.
///
/// Returns `None` for types that cannot be represented in LLVM during Pass 2
/// (e.g., Vec, Distribution, Named, Parameterized).
///
/// Wrapper types (Refined, Linear, Timed, Sized, etc.) are peeled to their
/// base type — refinements are enforced by verification, not by codegen.
pub fn to_llvm_type<'ctx>(ty: &Type, context: &'ctx Context) -> Option<BasicTypeEnum<'ctx>> {
    match ty {
        Type::Void | Type::Unit => {
            // Represent void/unit as an empty struct (LLVM void can't be used as a value)
            Some(context.struct_type(&[], false).into())
        }

        Type::Bool => Some(context.bool_type().into()),

        Type::Int { width, .. } => Some(context.custom_width_int_type(*width as u32).into()),

        Type::Float { precision } => match precision {
            FloatPrecision::F16 => Some(context.f16_type().into()),
            FloatPrecision::F32 => Some(context.f32_type().into()),
            FloatPrecision::F64 => Some(context.f64_type().into()),
            FloatPrecision::F128 => Some(context.f128_type().into()),
        },

        Type::Fixed { total_bits, .. } => {
            // Fixed-point: integer representation with implicit fractional scaling
            Some(context.custom_width_int_type(*total_bits as u32).into())
        }

        Type::Tuple(fields) => {
            let field_types: Vec<BasicTypeEnum<'ctx>> = fields
                .iter()
                .map(|f| to_llvm_type(f, context))
                .collect::<Option<Vec<_>>>()?;
            Some(context.struct_type(&field_types, false).into())
        }

        Type::Record(fields) => {
            // Sorted field order (BTreeMap iteration order)
            let field_types: Vec<BasicTypeEnum<'ctx>> = fields
                .values()
                .map(|f| to_llvm_type(f, context))
                .collect::<Option<Vec<_>>>()?;
            Some(context.struct_type(&field_types, false).into())
        }

        Type::Array { element, length } => {
            let elem_ty = to_llvm_type(element, context)?;
            Some(elem_ty.array_type(*length as u32).into())
        }

        Type::Option(inner) => {
            // Tagged option: { i1, inner }
            let inner_ty = to_llvm_type(inner, context)?;
            let fields = [context.bool_type().into(), inner_ty];
            Some(context.struct_type(&fields, false).into())
        }

        Type::Variant(cases) => {
            // C-like tagged union: { tag, max_payload_sized_field }
            // Tag: i8 for <=256 variants, i32 otherwise
            let tag_ty: BasicTypeEnum<'ctx> = if cases.len() <= 256 {
                context.i8_type().into()
            } else {
                context.i32_type().into()
            };

            // Find largest case payload
            let mut max_size = 0u32;
            let mut max_payload_ty: Option<BasicTypeEnum<'ctx>> = None;
            for case_ty in cases.values() {
                if let Some(llvm_ty) = to_llvm_type(case_ty, context) {
                    // Approximate size by using the type itself
                    // For proper sizing we'd need the target data layout,
                    // but for the union we just pick the "widest" case
                    let size = approx_type_bits(&llvm_ty);
                    if size > max_size {
                        max_size = size;
                        max_payload_ty = Some(llvm_ty);
                    }
                } else {
                    return None;
                }
            }

            match max_payload_ty {
                Some(payload_ty) => {
                    let fields = [tag_ty, payload_ty];
                    Some(context.struct_type(&fields, false).into())
                }
                None => {
                    // All cases are unit/void — tag only
                    let fields = [tag_ty];
                    Some(context.struct_type(&fields, false).into())
                }
            }
        }

        // Wrapper types: peel to base
        Type::Refined { base, .. } => to_llvm_type(base, context),
        Type::Linear { inner, .. }
        | Type::Timed { inner, .. }
        | Type::Sized { inner, .. }
        | Type::Powered { inner, .. }
        | Type::Bandwidth { inner, .. }
        | Type::Posterior { inner, .. }
        | Type::Interval { inner, .. }
        | Type::Approximate { inner, .. } => to_llvm_type(inner, context),

        // Unsupported in Pass 2
        Type::Vec { .. } | Type::Distribution(_) | Type::Named(_) | Type::Parameterized { .. } => {
            None
        }
    }
}

/// Approximate the bit-size of an LLVM type (for variant payload comparison).
fn approx_type_bits(ty: &BasicTypeEnum<'_>) -> u32 {
    match ty {
        BasicTypeEnum::IntType(t) => t.get_bit_width(),
        BasicTypeEnum::FloatType(_) => 64, // conservative
        BasicTypeEnum::StructType(t) => t.count_fields() * 64, // rough
        BasicTypeEnum::ArrayType(t) => t.len() * 64,
        BasicTypeEnum::PointerType(_) => 64,
        BasicTypeEnum::VectorType(t) => t.get_size() * 64,
        #[allow(unreachable_patterns)]
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use torc_core::types::{Linearity, Predicate};

    #[test]
    fn primitive_types() {
        let ctx = Context::create();

        // Bool -> i1
        let bool_ty = to_llvm_type(&Type::Bool, &ctx).unwrap();
        assert!(bool_ty.is_int_type());

        // i32 -> i32
        let i32_ty = to_llvm_type(&Type::i32(), &ctx).unwrap();
        assert!(i32_ty.is_int_type());
        assert_eq!(i32_ty.into_int_type().get_bit_width(), 32);

        // u64 -> i64 (LLVM doesn't distinguish signedness)
        let u64_ty = to_llvm_type(&Type::u64(), &ctx).unwrap();
        assert!(u64_ty.is_int_type());
        assert_eq!(u64_ty.into_int_type().get_bit_width(), 64);

        // f64 -> double
        let f64_ty = to_llvm_type(&Type::f64(), &ctx).unwrap();
        assert!(f64_ty.is_float_type());
    }

    #[test]
    fn void_unit_types() {
        let ctx = Context::create();

        let void_ty = to_llvm_type(&Type::Void, &ctx).unwrap();
        assert!(void_ty.is_struct_type());
        assert_eq!(void_ty.into_struct_type().count_fields(), 0);

        let unit_ty = to_llvm_type(&Type::Unit, &ctx).unwrap();
        assert!(unit_ty.is_struct_type());
    }

    #[test]
    fn tuple_struct_mapping() {
        let ctx = Context::create();

        let tuple = Type::Tuple(vec![Type::i32(), Type::f64()]);
        let llvm_ty = to_llvm_type(&tuple, &ctx).unwrap();
        assert!(llvm_ty.is_struct_type());
        assert_eq!(llvm_ty.into_struct_type().count_fields(), 2);
    }

    #[test]
    fn array_mapping() {
        let ctx = Context::create();

        let array = Type::Array {
            element: Box::new(Type::i32()),
            length: 10,
        };
        let llvm_ty = to_llvm_type(&array, &ctx).unwrap();
        assert!(llvm_ty.is_array_type());
        assert_eq!(llvm_ty.into_array_type().len(), 10);
    }

    #[test]
    fn wrapper_types_peel() {
        let ctx = Context::create();

        // Refined<i32> -> i32
        let refined = Type::i32().refined(Predicate::positive("value"));
        let llvm_ty = to_llvm_type(&refined, &ctx).unwrap();
        assert!(llvm_ty.is_int_type());
        assert_eq!(llvm_ty.into_int_type().get_bit_width(), 32);

        // Linear<f64> -> f64
        let linear = Type::f64().with_linearity(Linearity::Linear);
        let llvm_ty = to_llvm_type(&linear, &ctx).unwrap();
        assert!(llvm_ty.is_float_type());

        // Timed<Sized<i32>> -> i32
        let nested = Type::i32().sized(4).timed(100, "test");
        let llvm_ty = to_llvm_type(&nested, &ctx).unwrap();
        assert!(llvm_ty.is_int_type());
    }

    #[test]
    fn unsupported_types_return_none() {
        let ctx = Context::create();

        assert!(to_llvm_type(
            &Type::Vec {
                element: Box::new(Type::i32())
            },
            &ctx
        )
        .is_none());
        assert!(to_llvm_type(&Type::Distribution(Box::new(Type::f32())), &ctx).is_none());
        assert!(to_llvm_type(&Type::Named("Foo".into()), &ctx).is_none());
    }

    #[test]
    fn record_mapping() {
        let ctx = Context::create();
        let mut fields = BTreeMap::new();
        fields.insert("x".into(), Type::f32());
        fields.insert("y".into(), Type::f32());
        let record = Type::Record(fields);
        let llvm_ty = to_llvm_type(&record, &ctx).unwrap();
        assert!(llvm_ty.is_struct_type());
        assert_eq!(llvm_ty.into_struct_type().count_fields(), 2);
    }

    #[test]
    fn option_type_mapping() {
        let ctx = Context::create();
        let opt = Type::Option(Box::new(Type::i32()));
        let llvm_ty = to_llvm_type(&opt, &ctx).unwrap();
        assert!(llvm_ty.is_struct_type());
        // { i1, i32 }
        assert_eq!(llvm_ty.into_struct_type().count_fields(), 2);
    }

    #[test]
    fn fixed_point_mapping() {
        let ctx = Context::create();
        let fixed = Type::Fixed {
            total_bits: 16,
            frac_bits: 8,
        };
        let llvm_ty = to_llvm_type(&fixed, &ctx).unwrap();
        assert!(llvm_ty.is_int_type());
        assert_eq!(llvm_ty.into_int_type().get_bit_width(), 16);
    }
}
