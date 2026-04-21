//! Theme service — per-repo and global color theming via git refs.
//!
//! Themes are stored as JSON in `refs/adp/theme`:
//! ```json
//! { "primary": "#ff6600", "background": "#1a1a2e" }
//! ```
//!
//! Allowed keys: `primary`, `background`, `surface`, `text`, `border`, `accent`.
//! Allowed values: hex colors only (`#RGB`, `#RRGGBB`, `#RRGGBBAA`).
//!
//! Write path is git push only — the receive hook validates. No PUT/DELETE API.
//! Read path validates on load for defense in depth.

use crate::adp::storage::AdpStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Git ref name for theme data.
pub const THEME_REF: &str = "refs/adp/theme";

/// Maximum theme JSON payload size (2 KB).
const MAX_THEME_SIZE: usize = 2048;

/// Allowed color property names.
pub const ALLOWED_KEYS: &[&str] = &[
    "primary",
    "background",
    "surface",
    "text",
    "border",
    "accent",
];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("gix error: {0}")]
    Git(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A validated theme — a map of color names to hex values.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Theme {
    pub colors: HashMap<String, String>,
}

/// The merged theme result returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedTheme {
    /// Global theme colors (from _adp/system).
    pub global: HashMap<String, String>,
    /// Per-repo overrides.
    pub repo: HashMap<String, String>,
    /// Merged result: global + repo (repo wins on conflict).
    pub merged: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

pub struct ThemeValidator;

impl ThemeValidator {
    /// Validate a theme map. All keys must be in ALLOWED_KEYS, all values
    /// must be hex colors (`#RGB`, `#RRGGBB`, `#RRGGBBAA`).
    pub fn validate(colors: &HashMap<String, String>) -> Result<Theme, ThemeError> {
        // Size cap
        let json_size = serde_json::to_vec(colors)
            .map_err(|e| ThemeError::Validation(format!("serialization error: {e}")))?
            .len();
        if json_size > MAX_THEME_SIZE {
            return Err(ThemeError::Validation(format!(
                "theme payload exceeds {} byte limit (got {})",
                MAX_THEME_SIZE, json_size
            )));
        }

        for (key, value) in colors {
            // Key whitelist
            if !ALLOWED_KEYS.contains(&key.as_str()) {
                return Err(ThemeError::Validation(format!(
                    "unknown color key '{}'. Allowed: {:?}",
                    key, ALLOWED_KEYS
                )));
            }

            // Value: hex color only
            Self::validate_hex_color(key, value)?;
        }

        Ok(Theme {
            colors: colors.clone(),
        })
    }

    /// Validate a single hex color value.
    pub fn validate_hex_color(key: &str, value: &str) -> Result<(), ThemeError> {
        if !value.starts_with('#') {
            return Err(ThemeError::Validation(format!(
                "color '{}' = '{}': must start with '#'",
                key, value
            )));
        }

        let hex = &value[1..];
        let len = hex.len();

        if !matches!(len, 3 | 4 | 6 | 8) {
            return Err(ThemeError::Validation(format!(
                "color '{}' = '{}': hex must be 3, 4, 6, or 8 digits (#RGB, #RGBA, #RRGGBB, #RRGGBBAA)",
                key, value
            )));
        }

        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ThemeError::Validation(format!(
                "color '{}' = '{}': must be hexadecimal digits only",
                key, value
            )));
        }

        Ok(())
    }

    /// Validate raw push blob bytes as a theme JSON.
    /// Used by the receive-pack hook to reject invalid theme pushes.
    pub fn validate_push_blob(content: &[u8]) -> Result<(), ThemeError> {
        let colors: HashMap<String, String> =
            serde_json::from_slice(content).map_err(ThemeError::Json)?;
        Self::validate(&colors)?;
        Ok(())
    }

    /// Map a theme key to its CSS custom property name.
    /// `primary` → `--adp-primary`
    pub fn to_css_var(key: &str) -> String {
        format!("--adp-{}", key)
    }

    /// Convert a theme to CSS custom property key-value pairs.
    pub fn to_css_vars(theme: &Theme) -> HashMap<String, String> {
        theme
            .colors
            .iter()
            .map(|(k, v)| (Self::to_css_var(k), v.clone()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct ThemeService {
    storage_root: std::path::PathBuf,
}

impl ThemeService {
    pub fn new(storage_root: std::path::PathBuf) -> Self {
        Self { storage_root }
    }

    /// Get the storage root path.
    pub fn storage_root(&self) -> &Path {
        &self.storage_root
    }

    /// Get the global theme from `_adp/system`.
    pub fn get_global_theme(&self) -> Result<Theme, ThemeError> {
        let system_repo = self.storage_root.join("_adp/system");
        match AdpStore::read_ref_blob(&system_repo, THEME_REF) {
            Ok(value) => {
                let colors: HashMap<String, String> =
                    serde_json::from_value(value).map_err(ThemeError::Json)?;
                ThemeValidator::validate(&colors)
            }
            Err(_) => Ok(Theme::default()),
        }
    }

    /// Get the per-repo theme.
    pub fn get_repo_theme(&self, repo_path: &Path) -> Result<Theme, ThemeError> {
        match AdpStore::read_ref_blob(repo_path, THEME_REF) {
            Ok(value) => {
                let colors: HashMap<String, String> =
                    serde_json::from_value(value).map_err(ThemeError::Json)?;
                ThemeValidator::validate(&colors)
            }
            Err(_) => Ok(Theme::default()),
        }
    }

    /// Get the merged theme (global + per-repo, repo wins).
    pub fn get_merged_theme(&self, repo_path: &Path) -> Result<MergedTheme, ThemeError> {
        let global = self.get_global_theme()?;
        let repo = self.get_repo_theme(repo_path)?;

        let mut merged = global.colors.clone();
        for (k, v) in &repo.colors {
            merged.insert(k.clone(), v.clone());
        }

        Ok(MergedTheme {
            global: global.colors,
            repo: repo.colors,
            merged,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Validator: valid inputs ──

    #[test]
    fn test_valid_all_keys() {
        let colors = HashMap::from([
            ("primary".to_string(), "#ff6600".to_string()),
            ("background".to_string(), "#1a1a2e".to_string()),
            ("surface".to_string(), "#16213e".to_string()),
            ("text".to_string(), "#e0e0e0".to_string()),
            ("border".to_string(), "#2a2a4a".to_string()),
            ("accent".to_string(), "#0f3460".to_string()),
        ]);
        let theme = ThemeValidator::validate(&colors).unwrap();
        assert_eq!(theme.colors.len(), 6);
    }

    #[test]
    fn test_valid_partial_theme() {
        let colors = HashMap::from([("primary".to_string(), "#f60".to_string())]);
        let theme = ThemeValidator::validate(&colors).unwrap();
        assert_eq!(theme.colors["primary"], "#f60");
    }

    #[test]
    fn test_valid_hex_formats() {
        for val in &["#f60", "#f60a", "#ff6600", "#ff660080"] {
            let colors = HashMap::from([("primary".to_string(), val.to_string())]);
            assert!(
                ThemeValidator::validate(&colors).is_ok(),
                "should accept {}",
                val
            );
        }
    }

    // ── Validator: rejections ──

    #[test]
    fn test_reject_unknown_key() {
        let colors = HashMap::from([("evil".to_string(), "#ff0000".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("unknown color key"));
    }

    #[test]
    fn test_reject_named_color() {
        let colors = HashMap::from([("primary".to_string(), "red".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("must start with '#'"));
    }

    #[test]
    fn test_reject_css_function() {
        let colors = HashMap::from([("primary".to_string(), "var(--x)".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("must start with '#'"));
    }

    #[test]
    fn test_reject_url() {
        let colors = HashMap::from([("primary".to_string(), "url(http://evil.com)".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("must start with '#'"));
    }

    #[test]
    fn test_reject_wrong_hex_length() {
        let colors = HashMap::from([("primary".to_string(), "#ff660".to_string())]); // 5 digits
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("hex must be 3, 4, 6, or 8 digits"));
    }

    #[test]
    fn test_reject_non_hex_chars() {
        let colors = HashMap::from([("primary".to_string(), "#gggggg".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("hexadecimal digits only"));
    }

    #[test]
    fn test_reject_empty_value() {
        let colors = HashMap::from([("primary".to_string(), "".to_string())]);
        let err = ThemeValidator::validate(&colors).unwrap_err();
        assert!(err.to_string().contains("must start with '#'"));
    }

    // ── CSS var mapping ──

    #[test]
    fn test_to_css_var() {
        assert_eq!(ThemeValidator::to_css_var("primary"), "--adp-primary");
        assert_eq!(ThemeValidator::to_css_var("background"), "--adp-background");
    }

    #[test]
    fn test_to_css_vars() {
        let theme = Theme {
            colors: HashMap::from([
                ("primary".to_string(), "#ff6600".to_string()),
                ("border".to_string(), "#2a2a4a".to_string()),
            ]),
        };
        let vars = ThemeValidator::to_css_vars(&theme);
        assert_eq!(vars["--adp-primary"], "#ff6600");
        assert_eq!(vars["--adp-border"], "#2a2a4a");
    }

    // ── Merge logic ──

    #[test]
    fn test_merge_repo_overrides_global() {
        let global = Theme {
            colors: HashMap::from([
                ("primary".to_string(), "#0000ff".to_string()),
                ("text".to_string(), "#ffffff".to_string()),
            ]),
        };
        let repo = Theme {
            colors: HashMap::from([("primary".to_string(), "#ff6600".to_string())]),
        };

        let mut merged = global.colors.clone();
        for (k, v) in &repo.colors {
            merged.insert(k.clone(), v.clone());
        }

        assert_eq!(merged["primary"], "#ff6600"); // repo wins
        assert_eq!(merged["text"], "#ffffff"); // from global
    }

    // ── Service integration ──

    #[test]
    fn test_service_roundtrip_repo_theme() {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();
        gix::init_bare(&system_repo).unwrap();

        let repo_path = storage_root.join("acme/my-repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        gix::init_bare(&repo_path).unwrap();

        let service = ThemeService::new(storage_root);

        // No theme yet → default (empty)
        let theme = service.get_repo_theme(&repo_path).unwrap();
        assert!(theme.colors.is_empty());

        // Write theme directly via AdpStore (simulates git push)
        let colors = serde_json::json!({
            "primary": "#ff6600",
            "background": "#1a1a2e"
        });
        AdpStore::write_ref_blob(&repo_path, THEME_REF, &colors).unwrap();

        // Read back via service
        let theme = service.get_repo_theme(&repo_path).unwrap();
        assert_eq!(theme.colors["primary"], "#ff6600");
        assert_eq!(theme.colors["background"], "#1a1a2e");
    }

    #[test]
    fn test_service_merged_theme() {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let system_repo = storage_root.join("_adp/system");
        std::fs::create_dir_all(&system_repo).unwrap();
        gix::init_bare(&system_repo).unwrap();

        let repo_path = storage_root.join("acme/my-repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        gix::init_bare(&repo_path).unwrap();

        let service = ThemeService::new(storage_root);

        // Write global theme
        let global_colors = serde_json::json!({
            "primary": "#0000ff",
            "text": "#ffffff"
        });
        AdpStore::write_ref_blob(&system_repo, THEME_REF, &global_colors).unwrap();

        // Write repo theme (overrides primary)
        let repo_colors = serde_json::json!({"primary": "#ff6600"});
        AdpStore::write_ref_blob(&repo_path, THEME_REF, &repo_colors).unwrap();

        let merged = service.get_merged_theme(&repo_path).unwrap();
        assert_eq!(merged.global["primary"], "#0000ff");
        assert_eq!(merged.repo["primary"], "#ff6600");
        assert_eq!(merged.merged["primary"], "#ff6600"); // repo wins
        assert_eq!(merged.merged["text"], "#ffffff"); // from global
    }

    #[test]
    fn test_service_rejects_invalid_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let storage_root = dir.path().to_path_buf();
        let repo_path = storage_root.join("acme/my-repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        gix::init_bare(&repo_path).unwrap();

        let service = ThemeService::new(storage_root);

        // Write invalid theme directly
        let evil = serde_json::json!({"primary": "url(http://evil.com)"});
        AdpStore::write_ref_blob(&repo_path, THEME_REF, &evil).unwrap();

        // Service rejects on read
        let err = service.get_repo_theme(&repo_path).unwrap_err();
        assert!(err.to_string().contains("must start with '#'"));
    }
}
