use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsPointer {
    pub version: String,
    pub oid: String,
    pub size: u64,
}

#[derive(Debug)]
pub enum PointerParseError {
    InvalidFormat(String),
    MissingField(String),
    InvalidSize(String),
}

impl fmt::Display for PointerParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PointerParseError::InvalidFormat(msg) => write!(f, "Invalid pointer format: {}", msg),
            PointerParseError::MissingField(field) => {
                write!(f, "Missing required field: {}", field)
            }
            PointerParseError::InvalidSize(msg) => write!(f, "Invalid size: {}", msg),
        }
    }
}

impl std::error::Error for PointerParseError {}

impl LfsPointer {
    pub fn parse(content: &str) -> Result<Self, PointerParseError> {
        let mut version = None;
        let mut oid = None;
        let mut size = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once(' ') {
                match key {
                    "version" => version = Some(value.to_string()),
                    "oid" => {
                        if let Some(hash) = value.strip_prefix("sha256:") {
                            oid = Some(hash.to_string());
                        } else {
                            return Err(PointerParseError::InvalidFormat(
                                "OID must be prefixed with sha256:".to_string(),
                            ));
                        }
                    }
                    "size" => {
                        size = Some(value.parse::<u64>().map_err(|e| {
                            PointerParseError::InvalidSize(format!("Cannot parse size: {}", e))
                        })?);
                    }
                    _ => {}
                }
            }
        }

        let version =
            version.ok_or_else(|| PointerParseError::MissingField("version".to_string()))?;
        let oid = oid.ok_or_else(|| PointerParseError::MissingField("oid".to_string()))?;
        let size = size.ok_or_else(|| PointerParseError::MissingField("size".to_string()))?;

        Ok(LfsPointer { version, oid, size })
    }

    pub fn to_display_string(&self) -> String {
        format!(
            "version {}\noid sha256:{}\nsize {}\n",
            self.version, self.oid, self.size
        )
    }

    pub fn is_valid_oid(&self) -> bool {
        self.oid.len() == 64 && self.oid.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_pointer() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
        let pointer = LfsPointer::parse(content).unwrap();
        assert_eq!(pointer.version, "https://git-lfs.github.com/spec/v1");
        assert_eq!(
            pointer.oid,
            "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393"
        );
        assert_eq!(pointer.size, 12345);
    }

    #[test]
    fn test_parse_missing_field() {
        let content = "version https://git-lfs.github.com/spec/v1\noid sha256:abc123\n";
        let result = LfsPointer::parse(content);
        assert!(matches!(result, Err(PointerParseError::MissingField(_))));
    }

    #[test]
    fn test_is_valid_oid() {
        let pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".to_string(),
            oid: "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393".to_string(),
            size: 12345,
        };
        assert!(pointer.is_valid_oid());

        let invalid_pointer = LfsPointer {
            version: "https://git-lfs.github.com/spec/v1".to_string(),
            oid: "invalid".to_string(),
            size: 12345,
        };
        assert!(!invalid_pointer.is_valid_oid());
    }
}
