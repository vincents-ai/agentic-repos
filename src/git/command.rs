use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[derive(Debug, Clone, PartialEq)]
pub enum GitCommand {
    UploadPack { repository: String },
    ReceivePack { repository: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandValidationError {
    InvalidFormat,
    UnsafeRepository,
    UnsupportedCommand,
    EmptyRepository,
    InvalidCharacters,
}

impl std::fmt::Display for CommandValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandValidationError::InvalidFormat => write!(f, "Invalid command format"),
            CommandValidationError::UnsafeRepository => write!(f, "Unsafe repository path"),
            CommandValidationError::UnsupportedCommand => write!(f, "Unsupported command"),
            CommandValidationError::EmptyRepository => write!(f, "Empty repository name"),
            CommandValidationError::InvalidCharacters => {
                write!(f, "Invalid characters in repository name")
            }
        }
    }
}

impl std::error::Error for CommandValidationError {}

pub struct CommandValidator;

impl CommandValidator {
    /// Validate and parse SSH Git command
    pub fn validate_command(command: &str) -> Result<GitCommand, CommandValidationError> {
        let trimmed = command.trim();
        debug!("Validating command: {}", trimmed);

        if trimmed.is_empty() {
            return Err(CommandValidationError::InvalidFormat);
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.len() != 2 {
            warn!(
                "Invalid command format: expected 2 parts, got {}",
                parts.len()
            );
            return Err(CommandValidationError::InvalidFormat);
        }

        let git_command = parts[0];
        let repo_arg = parts[1];

        let repository = Self::validate_repository_path(repo_arg)?;

        match git_command {
            "git-upload-pack" => Ok(GitCommand::UploadPack { repository }),
            "git-receive-pack" => Ok(GitCommand::ReceivePack { repository }),
            _ => {
                warn!("Unsupported command: {}", git_command);
                Err(CommandValidationError::UnsupportedCommand)
            }
        }
    }

    /// Validate repository path for security
    fn validate_repository_path(repo_arg: &str) -> Result<String, CommandValidationError> {
        if repo_arg.is_empty() {
            return Err(CommandValidationError::EmptyRepository);
        }

        let cleaned = repo_arg.trim_matches('\'').trim_matches('"');

        if cleaned.is_empty() {
            return Err(CommandValidationError::EmptyRepository);
        }

        if !Self::is_safe_repository_name(cleaned) {
            warn!("Unsafe repository path detected: {}", cleaned);
            return Err(CommandValidationError::UnsafeRepository);
        }

        if !Self::has_valid_characters(cleaned) {
            warn!("Invalid characters in repository path: {}", cleaned);
            return Err(CommandValidationError::InvalidCharacters);
        }

        Ok(cleaned.to_string())
    }

    /// Check if repository name is safe (no path traversal)
    fn is_safe_repository_name(name: &str) -> bool {
        let path = Path::new(name);

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => return false,
                std::path::Component::CurDir => return false,
                std::path::Component::RootDir if path.components().count() > 1 => return false,
                std::path::Component::Normal(s) => {
                    if s.to_string_lossy().contains("..") {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        !name.starts_with('/')
            && !name.starts_with("..")
            && !name.contains("/../")
            && !name.contains("/./")
            && !name.ends_with("/..")
    }

    /// Validate characters in repository name
    fn has_valid_characters(name: &str) -> bool {
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '+'))
            && !name.chars().any(|c| c.is_control())
    }

    /// Extract tenant and repository from path
    pub fn parse_repository_path(repo_path: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = repo_path.trim_start_matches('/').split('/').collect();

        if parts.len() >= 2 {
            let tenant = parts[0].to_string();
            let repo_name = parts[1..].join("/");
            Some((tenant, repo_name))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_upload_pack_command() {
        let result = CommandValidator::validate_command("git-upload-pack tenant/repo.git");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            GitCommand::UploadPack {
                repository: "tenant/repo.git".to_string()
            }
        );
    }

    #[test]
    fn test_valid_receive_pack_command() {
        let result = CommandValidator::validate_command("git-receive-pack 'tenant/repo.git'");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            GitCommand::ReceivePack {
                repository: "tenant/repo.git".to_string()
            }
        );
    }

    #[test]
    fn test_invalid_command_format() {
        let result = CommandValidator::validate_command("git-upload-pack");
        assert!(matches!(result, Err(CommandValidationError::InvalidFormat)));
    }

    #[test]
    fn test_unsupported_command() {
        let result = CommandValidator::validate_command("rm valid/repo.git");
        assert!(matches!(
            result,
            Err(CommandValidationError::UnsupportedCommand)
        ));

        let result2 = CommandValidator::validate_command("delete valid/repo.git");
        assert!(matches!(
            result2,
            Err(CommandValidationError::UnsupportedCommand)
        ));
    }

    #[test]
    fn test_path_traversal_protection() {
        let test_cases = vec![
            "git-upload-pack ../../../etc/passwd",
            "git-upload-pack tenant/../../../etc/passwd",
            "git-upload-pack tenant/./repo.git",
            "git-upload-pack /absolute/path/repo.git",
            "git-upload-pack tenant/repo/../config",
        ];

        for test_case in test_cases {
            let result = CommandValidator::validate_command(test_case);
            assert!(result.is_err(), "Should reject: {}", test_case);
        }
    }

    #[test]
    fn test_invalid_characters() {
        let test_cases = vec![
            "git-upload-pack tenant/repo;ls",
            "git-upload-pack tenant/repo&echo",
            "git-upload-pack tenant/repo|cat",
            "git-upload-pack tenant/repo`whoami`",
            "git-upload-pack tenant/repo$(id)",
            "git-upload-pack tenant/repo\x00null",
        ];

        for test_case in test_cases {
            let result = CommandValidator::validate_command(test_case);
            assert!(result.is_err(), "Should reject: {}", test_case);
        }
    }

    #[test]
    fn test_repository_path_parsing() {
        assert_eq!(
            CommandValidator::parse_repository_path("tenant/repo.git"),
            Some(("tenant".to_string(), "repo.git".to_string()))
        );

        assert_eq!(
            CommandValidator::parse_repository_path("/tenant/project/repo.git"),
            Some(("tenant".to_string(), "project/repo.git".to_string()))
        );

        assert_eq!(CommandValidator::parse_repository_path("single"), None);
    }

    #[test]
    fn test_safe_repository_names() {
        let valid_names = vec![
            "tenant/repo.git",
            "org/project-name/repo.git",
            "user_name/repo_name.git",
            "tenant/subdir/project+fork.git",
        ];

        for name in valid_names {
            assert!(
                CommandValidator::is_safe_repository_name(name),
                "Should accept: {}",
                name
            );
        }
    }

    #[test]
    fn test_empty_repository_rejection() {
        let result = CommandValidator::validate_command("git-upload-pack ''");
        assert!(matches!(
            result,
            Err(CommandValidationError::EmptyRepository)
        ));

        let result = CommandValidator::validate_command("git-upload-pack \"\"");
        assert!(matches!(
            result,
            Err(CommandValidationError::EmptyRepository)
        ));
    }
}
