//! Data marshaling between C types and Torc types.
//!
//! Provides bidirectional mapping between C's type system and Torc's type system,
//! along with strategy selection for runtime data conversion.

use torc_core::types::Type;

use crate::csig::CType;
use crate::error::Result;

/// Strategy for marshaling data across the FFI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarshalStrategy {
    /// Direct bit-compatible transfer (primitives, stdint types).
    Direct,
    /// Struct field layout with padding/alignment.
    StructLayout,
    /// Convert Torc string/array to null-terminated C string.
    StringToNullTerminated,
    /// Convert Torc array to (pointer, length) pair.
    ArrayToPtrLen,
    /// Pass as opaque pointer (void*, struct pointers).
    OpaquePointer,
    /// Convert nullable pointer to Torc Option type.
    NullableToOption,
}

/// Convert a C type to its corresponding Torc type.
///
/// Uses platform word size (from torc-targets) for size_t mapping.
pub fn torc_type_from_ctype(ct: &CType, word_bits: u8) -> Result<Type> {
    match ct {
        CType::Void => Ok(Type::Void),
        CType::Char | CType::SignedChar => Ok(Type::i8()),
        CType::UnsignedChar => Ok(Type::u8()),
        CType::Short => Ok(Type::i16()),
        CType::UnsignedShort => Ok(Type::u16()),
        CType::Int | CType::Long => Ok(Type::i32()),
        CType::UnsignedInt | CType::UnsignedLong => Ok(Type::u32()),
        CType::LongLong => Ok(Type::i64()),
        CType::UnsignedLongLong => Ok(Type::u64()),
        CType::Float => Ok(Type::f32()),
        CType::Double | CType::LongDouble => Ok(Type::f64()),
        CType::Bool => Ok(Type::Bool),
        CType::Int8 => Ok(Type::i8()),
        CType::Int16 => Ok(Type::i16()),
        CType::Int32 => Ok(Type::i32()),
        CType::Int64 => Ok(Type::i64()),
        CType::UInt8 => Ok(Type::u8()),
        CType::UInt16 => Ok(Type::u16()),
        CType::UInt32 => Ok(Type::u32()),
        CType::UInt64 => Ok(Type::u64()),
        CType::SizeT => {
            if word_bits >= 64 {
                Ok(Type::u64())
            } else {
                Ok(Type::u32())
            }
        }
        CType::Pointer(inner) => {
            match inner.as_ref() {
                CType::Void => {
                    // void* → u64 (opaque pointer)
                    Ok(Type::u64())
                }
                CType::Const(inner2) if matches!(inner2.as_ref(), CType::Char) => {
                    // const char* → Array(u8) (C string)
                    Ok(Type::Array {
                        element: Box::new(Type::u8()),
                        length: 0, // dynamic length
                    })
                }
                CType::Char => {
                    // char* → Array(u8) (mutable C string)
                    Ok(Type::Array {
                        element: Box::new(Type::u8()),
                        length: 0,
                    })
                }
                _ => {
                    // Other pointers → u64 (opaque pointer)
                    Ok(Type::u64())
                }
            }
        }
        CType::Const(inner) => torc_type_from_ctype(inner, word_bits),
        CType::OpaqueStruct(_) => Ok(Type::u64()), // struct pointer → opaque
    }
}

/// Convert a Torc type to a C type string for header generation.
pub fn ctype_string_from_torc(ty: &Type) -> String {
    match ty {
        Type::Void => "void".to_string(),
        Type::Unit => "void".to_string(),
        Type::Bool => "_Bool".to_string(),
        Type::Int { width: 8, signedness } => {
            if matches!(signedness, torc_core::types::Signedness::Unsigned) {
                "uint8_t".to_string()
            } else {
                "int8_t".to_string()
            }
        }
        Type::Int { width: 16, signedness } => {
            if matches!(signedness, torc_core::types::Signedness::Unsigned) {
                "uint16_t".to_string()
            } else {
                "int16_t".to_string()
            }
        }
        Type::Int { width: 32, signedness } => {
            if matches!(signedness, torc_core::types::Signedness::Unsigned) {
                "uint32_t".to_string()
            } else {
                "int32_t".to_string()
            }
        }
        Type::Int { width: 64, signedness } => {
            if matches!(signedness, torc_core::types::Signedness::Unsigned) {
                "uint64_t".to_string()
            } else {
                "int64_t".to_string()
            }
        }
        Type::Float { precision } => match precision {
            torc_core::types::FloatPrecision::F32 => "float".to_string(),
            torc_core::types::FloatPrecision::F64 => "double".to_string(),
            _ => "double".to_string(),
        },
        Type::Array { element, .. } => {
            // Array<u8> → const char* (string convention)
            if matches!(element.as_ref(), Type::Int { width: 8, .. }) {
                "const char*".to_string()
            } else {
                format!("{}*", ctype_string_from_torc(element))
            }
        }
        Type::Option(inner) => {
            // Option<T> → nullable pointer
            let inner_c = ctype_string_from_torc(inner);
            if inner_c.ends_with('*') {
                inner_c
            } else {
                format!("{inner_c}*")
            }
        }
        Type::Refined { base, .. } => ctype_string_from_torc(base),
        Type::Linear { inner, .. } => ctype_string_from_torc(inner),
        Type::Timed { inner, .. } => ctype_string_from_torc(inner),
        Type::Sized { inner, .. } => ctype_string_from_torc(inner),
        Type::Named(name) => format!("struct {name}"),
        _ => "void*".to_string(), // fallback for complex types
    }
}

/// Select the appropriate marshal strategy for a C type.
pub fn select_strategy(ct: &CType) -> MarshalStrategy {
    match ct {
        CType::Void | CType::Char | CType::SignedChar | CType::UnsignedChar
        | CType::Short | CType::UnsignedShort | CType::Int | CType::UnsignedInt
        | CType::Long | CType::UnsignedLong | CType::LongLong | CType::UnsignedLongLong
        | CType::Float | CType::Double | CType::LongDouble | CType::Bool
        | CType::Int8 | CType::Int16 | CType::Int32 | CType::Int64
        | CType::UInt8 | CType::UInt16 | CType::UInt32 | CType::UInt64
        | CType::SizeT => MarshalStrategy::Direct,

        CType::Pointer(inner) => match inner.as_ref() {
            CType::Void => MarshalStrategy::OpaquePointer,
            CType::Const(inner2) if matches!(inner2.as_ref(), CType::Char) => {
                MarshalStrategy::StringToNullTerminated
            }
            CType::Char => MarshalStrategy::StringToNullTerminated,
            CType::OpaqueStruct(_) => MarshalStrategy::OpaquePointer,
            _ => MarshalStrategy::OpaquePointer,
        },

        CType::Const(inner) => select_strategy(inner),
        CType::OpaqueStruct(_) => MarshalStrategy::StructLayout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_primitives() {
        assert_eq!(torc_type_from_ctype(&CType::Int, 64).unwrap(), Type::i32());
        assert_eq!(torc_type_from_ctype(&CType::Float, 64).unwrap(), Type::f32());
        assert_eq!(torc_type_from_ctype(&CType::Double, 64).unwrap(), Type::f64());
        assert_eq!(torc_type_from_ctype(&CType::Char, 64).unwrap(), Type::i8());
        assert_eq!(torc_type_from_ctype(&CType::Bool, 64).unwrap(), Type::Bool);
    }

    #[test]
    fn map_stdint() {
        assert_eq!(torc_type_from_ctype(&CType::Int32, 64).unwrap(), Type::i32());
        assert_eq!(torc_type_from_ctype(&CType::UInt8, 64).unwrap(), Type::u8());
        assert_eq!(torc_type_from_ctype(&CType::Int64, 64).unwrap(), Type::i64());
        assert_eq!(torc_type_from_ctype(&CType::UInt16, 64).unwrap(), Type::u16());
    }

    #[test]
    fn map_void_pointer() {
        let ct = CType::Pointer(Box::new(CType::Void));
        assert_eq!(torc_type_from_ctype(&ct, 64).unwrap(), Type::u64());
    }

    #[test]
    fn map_const_char_pointer() {
        let ct = CType::Pointer(Box::new(CType::Const(Box::new(CType::Char))));
        let ty = torc_type_from_ctype(&ct, 64).unwrap();
        match &ty {
            Type::Array { element, length } => {
                assert_eq!(element.as_ref(), &Type::u8());
                assert_eq!(*length, 0);
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn map_size_t_by_platform() {
        assert_eq!(torc_type_from_ctype(&CType::SizeT, 64).unwrap(), Type::u64());
        assert_eq!(torc_type_from_ctype(&CType::SizeT, 32).unwrap(), Type::u32());
    }

    #[test]
    fn torc_to_c_primitives() {
        assert_eq!(ctype_string_from_torc(&Type::i32()), "int32_t");
        assert_eq!(ctype_string_from_torc(&Type::f64()), "double");
        assert_eq!(ctype_string_from_torc(&Type::u8()), "uint8_t");
        assert_eq!(ctype_string_from_torc(&Type::Void), "void");
        assert_eq!(ctype_string_from_torc(&Type::Bool), "_Bool");
    }

    #[test]
    fn strategy_selection() {
        assert_eq!(select_strategy(&CType::Int), MarshalStrategy::Direct);
        assert_eq!(select_strategy(&CType::Double), MarshalStrategy::Direct);
        assert_eq!(
            select_strategy(&CType::Pointer(Box::new(CType::Void))),
            MarshalStrategy::OpaquePointer
        );
        assert_eq!(
            select_strategy(&CType::Pointer(Box::new(CType::Const(Box::new(CType::Char))))),
            MarshalStrategy::StringToNullTerminated
        );
        assert_eq!(
            select_strategy(&CType::OpaqueStruct("foo".to_string())),
            MarshalStrategy::StructLayout
        );
    }
}
