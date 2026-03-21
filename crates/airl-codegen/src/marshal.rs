/// Error during marshaling.
#[derive(Debug)]
pub enum MarshalError {
    TypeMismatch { expected: String, got: String },
    UnsupportedType(String),
}

impl std::fmt::Display for MarshalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarshalError::TypeMismatch { expected, got } => {
                write!(f, "marshal type mismatch: expected {}, got {}", expected, got)
            }
            MarshalError::UnsupportedType(t) => write!(f, "unsupported type for JIT: {}", t),
        }
    }
}

/// A raw value that can be passed to/from native code.
/// Stored as a u64 bitpattern regardless of actual type.
#[derive(Debug, Clone, Copy)]
pub struct RawValue(pub u64);

impl RawValue {
    pub fn from_i32(v: i32) -> Self { Self(v as u64) }
    pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    pub fn from_f32(v: f32) -> Self { Self(f32::to_bits(v) as u64) }
    pub fn from_f64(v: f64) -> Self { Self(f64::to_bits(v)) }
    pub fn from_bool(v: bool) -> Self { Self(v as u64) }

    pub fn to_i32(self) -> i32 { self.0 as i32 }
    pub fn to_i64(self) -> i64 { self.0 as i64 }
    pub fn to_f32(self) -> f32 { f32::from_bits(self.0 as u32) }
    pub fn to_f64(self) -> f64 { f64::from_bits(self.0) }
    pub fn to_bool(self) -> bool { self.0 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_i64() {
        let v = RawValue::from_i64(42);
        assert_eq!(v.to_i64(), 42);
    }

    #[test]
    fn roundtrip_i64_negative() {
        let v = RawValue::from_i64(-7);
        assert_eq!(v.to_i64(), -7);
    }

    #[test]
    fn roundtrip_f64() {
        let v = RawValue::from_f64(3.14);
        assert!((v.to_f64() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn roundtrip_bool() {
        assert!(RawValue::from_bool(true).to_bool());
        assert!(!RawValue::from_bool(false).to_bool());
    }

    #[test]
    fn roundtrip_i32() {
        let v = RawValue::from_i32(99);
        assert_eq!(v.to_i32(), 99);
    }

    #[test]
    fn roundtrip_f32() {
        let v = RawValue::from_f32(2.5);
        assert!((v.to_f32() - 2.5).abs() < 1e-6);
    }
}
