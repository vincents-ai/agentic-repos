pub mod handlers;
pub mod rules;
pub mod scanner;
pub mod secret_patterns;

pub use rules::{
    load_rules_from_json, load_rules_from_yaml, scan_with_rules, CompiledRule, RuleMatch,
    SecurityRule,
};
pub use scanner::{ScanRequest, ScanResult, SecretMatch, SecretPattern, SecretScanner};
pub use secret_patterns::{builtin_patterns, SecretPatternDef, SecretPatternType, Severity};
