//! Hand-written C function signature parser.
//!
//! Handles common C function signatures including stdint types, const qualifiers,
//! pointer types, and variadic functions. Does NOT handle function pointers,
//! array parameters, or complex attributes.

use crate::error::{FfiError, Result};

/// A C type representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CType {
    Void,
    Char,
    SignedChar,
    UnsignedChar,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    Float,
    Double,
    LongDouble,
    // stdint types
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    SizeT,
    Bool,
    /// Pointer to another type.
    Pointer(Box<CType>),
    /// Const-qualified type.
    Const(Box<CType>),
    /// Opaque struct reference (by name).
    OpaqueStruct(String),
}

impl CType {
    /// Whether this type is void.
    pub fn is_void(&self) -> bool {
        matches!(self, CType::Void)
    }

    /// Strip const qualifiers from outer level.
    pub fn strip_const(&self) -> &CType {
        match self {
            CType::Const(inner) => inner.strip_const(),
            other => other,
        }
    }
}

impl std::fmt::Display for CType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CType::Void => write!(f, "void"),
            CType::Char => write!(f, "char"),
            CType::SignedChar => write!(f, "signed char"),
            CType::UnsignedChar => write!(f, "unsigned char"),
            CType::Short => write!(f, "short"),
            CType::UnsignedShort => write!(f, "unsigned short"),
            CType::Int => write!(f, "int"),
            CType::UnsignedInt => write!(f, "unsigned int"),
            CType::Long => write!(f, "long"),
            CType::UnsignedLong => write!(f, "unsigned long"),
            CType::LongLong => write!(f, "long long"),
            CType::UnsignedLongLong => write!(f, "unsigned long long"),
            CType::Float => write!(f, "float"),
            CType::Double => write!(f, "double"),
            CType::LongDouble => write!(f, "long double"),
            CType::Int8 => write!(f, "int8_t"),
            CType::Int16 => write!(f, "int16_t"),
            CType::Int32 => write!(f, "int32_t"),
            CType::Int64 => write!(f, "int64_t"),
            CType::UInt8 => write!(f, "uint8_t"),
            CType::UInt16 => write!(f, "uint16_t"),
            CType::UInt32 => write!(f, "uint32_t"),
            CType::UInt64 => write!(f, "uint64_t"),
            CType::SizeT => write!(f, "size_t"),
            CType::Bool => write!(f, "_Bool"),
            CType::Pointer(inner) => write!(f, "{inner}*"),
            CType::Const(inner) => write!(f, "const {inner}"),
            CType::OpaqueStruct(name) => write!(f, "struct {name}"),
        }
    }
}

/// A parsed C function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CParam {
    /// Parameter type.
    pub param_type: CType,
    /// Parameter name (may be empty if unnamed).
    pub name: String,
}

/// A parsed C function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSignature {
    /// Return type.
    pub return_type: CType,
    /// Function name.
    pub name: String,
    /// Parameters (excluding variadic `...`).
    pub parameters: Vec<CParam>,
    /// Whether the function is variadic (`...`).
    pub is_variadic: bool,
}

impl CSignature {
    /// Parse a C function signature string.
    ///
    /// Examples:
    /// - `"double sin(double x)"`
    /// - `"void* malloc(size_t size)"`
    /// - `"int printf(const char* fmt, ...)"`
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(FfiError::InvalidCSignature {
                detail: "empty signature".to_string(),
            });
        }

        // Find the opening parenthesis
        let paren_pos = input.find('(').ok_or_else(|| FfiError::InvalidCSignature {
            detail: "missing '('".to_string(),
        })?;

        // Ensure closing parenthesis
        if !input.ends_with(')') {
            return Err(FfiError::InvalidCSignature {
                detail: "missing ')'".to_string(),
            });
        }

        // Split into return_type+name and params
        let before_paren = input[..paren_pos].trim();
        let params_str = &input[paren_pos + 1..input.len() - 1];

        // Parse return type and function name from before_paren
        let (return_type, name) = parse_type_and_name(before_paren)?;

        // Parse parameters
        let (parameters, is_variadic) = parse_params(params_str)?;

        Ok(CSignature {
            return_type,
            name,
            parameters,
            is_variadic,
        })
    }
}

impl std::fmt::Display for CSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}(", self.return_type, self.name)?;
        for (i, param) in self.parameters.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param.param_type)?;
            if !param.name.is_empty() {
                write!(f, " {}", param.name)?;
            }
        }
        if self.is_variadic {
            if !self.parameters.is_empty() {
                write!(f, ", ")?;
            }
            write!(f, "...")?;
        }
        write!(f, ")")
    }
}

/// Parse a type specifier from a sequence of tokens.
fn parse_base_type(tokens: &[&str]) -> Result<(CType, usize)> {
    if tokens.is_empty() {
        return Err(FfiError::InvalidCSignature {
            detail: "expected type".to_string(),
        });
    }

    let mut pos = 0;
    let mut is_const = false;

    // Handle leading `const`
    if tokens[pos] == "const" {
        is_const = true;
        pos += 1;
        if pos >= tokens.len() {
            return Err(FfiError::InvalidCSignature {
                detail: "expected type after 'const'".to_string(),
            });
        }
    }

    // Handle `struct`
    if tokens[pos] == "struct" {
        pos += 1;
        if pos >= tokens.len() {
            return Err(FfiError::InvalidCSignature {
                detail: "expected struct name".to_string(),
            });
        }
        let name = tokens[pos].to_string();
        pos += 1;
        let ct = CType::OpaqueStruct(name);
        let ct = if is_const { CType::Const(Box::new(ct)) } else { ct };
        return Ok((ct, pos));
    }

    // Handle `unsigned`/`signed` modifiers
    let is_unsigned = tokens[pos] == "unsigned";
    let is_signed = tokens[pos] == "signed";

    if is_unsigned || is_signed {
        pos += 1;
        if pos >= tokens.len() {
            // bare `unsigned` or `signed` means `unsigned int` / `signed int`
            let ct = if is_unsigned { CType::UnsignedInt } else { CType::Int };
            let ct = if is_const { CType::Const(Box::new(ct)) } else { ct };
            return Ok((ct, pos));
        }

        let next = tokens[pos];
        let ct = match next {
            "char" => {
                pos += 1;
                if is_unsigned { CType::UnsignedChar } else { CType::SignedChar }
            }
            "short" => {
                pos += 1;
                if is_unsigned { CType::UnsignedShort } else { CType::Short }
            }
            "int" => {
                pos += 1;
                if is_unsigned { CType::UnsignedInt } else { CType::Int }
            }
            "long" => {
                pos += 1;
                // Check for `long long`
                if pos < tokens.len() && tokens[pos] == "long" {
                    pos += 1;
                    if is_unsigned { CType::UnsignedLongLong } else { CType::LongLong }
                } else if is_unsigned {
                    CType::UnsignedLong
                } else {
                    CType::Long
                }
            }
            _ => {
                // Just `unsigned` with no recognized type → unsigned int
                if is_unsigned { CType::UnsignedInt } else { CType::Int }
            }
        };
        let ct = if is_const { CType::Const(Box::new(ct)) } else { ct };
        return Ok((ct, pos));
    }

    // Simple type keywords
    let ct = match tokens[pos] {
        "void" => { pos += 1; CType::Void }
        "char" => { pos += 1; CType::Char }
        "short" => { pos += 1; CType::Short }
        "int" => { pos += 1; CType::Int }
        "long" => {
            pos += 1;
            if pos < tokens.len() && tokens[pos] == "long" {
                pos += 1;
                CType::LongLong
            } else if pos < tokens.len() && tokens[pos] == "double" {
                pos += 1;
                CType::LongDouble
            } else {
                CType::Long
            }
        }
        "float" => { pos += 1; CType::Float }
        "double" => { pos += 1; CType::Double }
        "_Bool" => { pos += 1; CType::Bool }
        "bool" => { pos += 1; CType::Bool }
        "size_t" => { pos += 1; CType::SizeT }
        "int8_t" => { pos += 1; CType::Int8 }
        "int16_t" => { pos += 1; CType::Int16 }
        "int32_t" => { pos += 1; CType::Int32 }
        "int64_t" => { pos += 1; CType::Int64 }
        "uint8_t" => { pos += 1; CType::UInt8 }
        "uint16_t" => { pos += 1; CType::UInt16 }
        "uint32_t" => { pos += 1; CType::UInt32 }
        "uint64_t" => { pos += 1; CType::UInt64 }
        other => {
            return Err(FfiError::InvalidCSignature {
                detail: format!("unknown type '{other}'"),
            });
        }
    };
    let ct = if is_const { CType::Const(Box::new(ct)) } else { ct };
    Ok((ct, pos))
}

/// Tokenize a C declaration fragment, splitting on whitespace but keeping `*` as separate tokens.
fn tokenize(s: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    for part in s.split_whitespace() {
        let mut remaining = part;
        while !remaining.is_empty() {
            if let Some(star_pos) = remaining.find('*') {
                if star_pos > 0 {
                    tokens.push(&remaining[..star_pos]);
                }
                tokens.push("*");
                remaining = &remaining[star_pos + 1..];
            } else {
                tokens.push(remaining);
                break;
            }
        }
    }
    tokens
}

/// Parse "return_type function_name" from the part before `(`.
fn parse_type_and_name(s: &str) -> Result<(CType, String)> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(FfiError::InvalidCSignature {
            detail: "empty return type and name".to_string(),
        });
    }

    // The last non-`*` token is the function name.
    // But we need to account for pointer stars being part of the return type.
    // Strategy: parse the type from the front, and whatever is left is the name.

    let (base_type, consumed) = parse_base_type(&tokens)?;

    // After the base type, collect pointer stars, then the last token is the name.
    let remaining = &tokens[consumed..];
    if remaining.is_empty() {
        return Err(FfiError::InvalidCSignature {
            detail: "missing function name".to_string(),
        });
    }

    // Count pointer stars and find the name
    let mut ptr_count = 0;
    let mut name_idx = None;
    for (i, tok) in remaining.iter().enumerate() {
        if *tok == "*" {
            ptr_count += 1;
        } else if *tok == "const" {
            // const after pointer: e.g., `char* const name` — skip
            continue;
        } else {
            name_idx = Some(i);
        }
    }

    let name = match name_idx {
        Some(idx) => remaining[idx].to_string(),
        None => {
            return Err(FfiError::InvalidCSignature {
                detail: "missing function name after type".to_string(),
            });
        }
    };

    // Wrap base type in Pointer layers
    let mut result_type = base_type;
    for _ in 0..ptr_count {
        result_type = CType::Pointer(Box::new(result_type));
    }

    Ok((result_type, name))
}

/// Parse a single parameter type (no name expected, but name tolerated).
fn parse_param_type(s: &str) -> Result<(CType, String)> {
    let s = s.trim();
    if s == "..." || s == "void" {
        // These are handled by the caller
        return Ok((CType::Void, String::new()));
    }

    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(FfiError::InvalidCSignature {
            detail: "empty parameter".to_string(),
        });
    }

    let (base_type, consumed) = parse_base_type(&tokens)?;
    let remaining = &tokens[consumed..];

    // Collect pointer stars and optional name
    let mut ptr_count = 0;
    let mut has_const_after_ptr = false;
    let mut name = String::new();

    for tok in remaining {
        if *tok == "*" {
            if has_const_after_ptr {
                // pointer to const pointer — just add another pointer level
                has_const_after_ptr = false;
            }
            ptr_count += 1;
        } else if *tok == "const" {
            has_const_after_ptr = true;
        } else {
            // Must be the parameter name
            name = tok.to_string();
        }
    }

    let mut result_type = base_type;
    for _ in 0..ptr_count {
        result_type = CType::Pointer(Box::new(result_type));
    }

    Ok((result_type, name))
}

/// Parse the parameter list between `(` and `)`.
fn parse_params(s: &str) -> Result<(Vec<CParam>, bool)> {
    let s = s.trim();

    // Handle `void` or empty parameter lists
    if s.is_empty() || s == "void" {
        return Ok((Vec::new(), false));
    }

    let parts: Vec<&str> = s.split(',').collect();
    let mut params = Vec::new();
    let mut is_variadic = false;

    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part == "..." {
            if i != parts.len() - 1 {
                return Err(FfiError::InvalidCSignature {
                    detail: "'...' must be the last parameter".to_string(),
                });
            }
            is_variadic = true;
            continue;
        }

        let (param_type, param_name) = parse_param_type(part)?;
        params.push(CParam {
            param_type,
            name: param_name,
        });
    }

    Ok((params, is_variadic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_function() {
        let sig = CSignature::parse("double sin(double x)").unwrap();
        assert_eq!(sig.name, "sin");
        assert_eq!(sig.return_type, CType::Double);
        assert_eq!(sig.parameters.len(), 1);
        assert_eq!(sig.parameters[0].param_type, CType::Double);
        assert_eq!(sig.parameters[0].name, "x");
        assert!(!sig.is_variadic);
    }

    #[test]
    fn parse_void_return() {
        let sig = CSignature::parse("void free(void* ptr)").unwrap();
        assert_eq!(sig.name, "free");
        assert_eq!(sig.return_type, CType::Void);
        assert_eq!(
            sig.parameters[0].param_type,
            CType::Pointer(Box::new(CType::Void))
        );
    }

    #[test]
    fn parse_pointer_return() {
        let sig = CSignature::parse("void* malloc(size_t size)").unwrap();
        assert_eq!(sig.name, "malloc");
        assert_eq!(
            sig.return_type,
            CType::Pointer(Box::new(CType::Void))
        );
        assert_eq!(sig.parameters[0].param_type, CType::SizeT);
    }

    #[test]
    fn parse_const_char_pointer() {
        let sig = CSignature::parse("int puts(const char* s)").unwrap();
        assert_eq!(sig.name, "puts");
        assert_eq!(sig.return_type, CType::Int);
        assert_eq!(
            sig.parameters[0].param_type,
            CType::Pointer(Box::new(CType::Const(Box::new(CType::Char))))
        );
    }

    #[test]
    fn parse_variadic() {
        let sig = CSignature::parse("int printf(const char* fmt, ...)").unwrap();
        assert_eq!(sig.name, "printf");
        assert!(sig.is_variadic);
        assert_eq!(sig.parameters.len(), 1);
    }

    #[test]
    fn parse_stdint_types() {
        let sig =
            CSignature::parse("int32_t foo(uint8_t a, int64_t b)").unwrap();
        assert_eq!(sig.name, "foo");
        assert_eq!(sig.return_type, CType::Int32);
        assert_eq!(sig.parameters[0].param_type, CType::UInt8);
        assert_eq!(sig.parameters[1].param_type, CType::Int64);
    }

    #[test]
    fn parse_multi_param() {
        let sig = CSignature::parse("double fma(double x, double y, double z)")
            .unwrap();
        assert_eq!(sig.name, "fma");
        assert_eq!(sig.parameters.len(), 3);
        for p in &sig.parameters {
            assert_eq!(p.param_type, CType::Double);
        }
    }

    #[test]
    fn parse_no_param_names() {
        let sig = CSignature::parse("float sqrtf(float)").unwrap();
        assert_eq!(sig.name, "sqrtf");
        assert_eq!(sig.parameters[0].param_type, CType::Float);
        assert!(sig.parameters[0].name.is_empty());
    }

    #[test]
    fn parse_void_params() {
        let sig = CSignature::parse("int getpid(void)").unwrap();
        assert_eq!(sig.name, "getpid");
        assert!(sig.parameters.is_empty());
    }

    #[test]
    fn parse_invalid_missing_paren() {
        assert!(CSignature::parse("double sin double x").is_err());
    }

    #[test]
    fn parse_empty() {
        assert!(CSignature::parse("").is_err());
    }
}
