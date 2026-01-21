#[cfg(test)]
mod web_feature_tests {
    use crate::features::FeatureFlags;
    use crate::web::WebUI;
    use std::collections::HashMap;

    #[test]
    fn test_web_ui_feature_flag_from_config() {
        let mut features = HashMap::new();
        features.insert("web_ui".to_string(), true);

        let flags = FeatureFlags::load_from_config(&features);

        assert!(flags.web_ui);
        assert!(!flags.ldap_auth);
        assert!(!flags.monitoring);
    }

    #[test]
    fn test_web_ui_feature_flag_disabled_by_default() {
        let flags = FeatureFlags::default();

        assert!(!flags.web_ui);
        assert!(!flags.ldap_auth);
        assert!(!flags.monitoring);
    }

    #[tokio::test]
    async fn test_web_ui_initialization() {
        use crate::git::{GitConfig, GitService};
        use crate::storage::local::LocalStorage;
        use std::path::PathBuf;
        use std::sync::Arc;

        let storage = Arc::new(LocalStorage::new(PathBuf::from("./test_repos")));
        let git_service = Arc::new(GitService::new(storage, Some(GitConfig::default())));

        let mut features = HashMap::new();
        features.insert("web_ui".to_string(), true);
        let feature_flags = FeatureFlags::load_from_config(&features);

        let web_ui_result = WebUI::new(git_service.clone(), feature_flags.clone());
        assert!(web_ui_result.is_ok());

        let disabled_flags = FeatureFlags::default();
        let disabled_web_ui = WebUI::new(git_service.clone(), disabled_flags);
        assert!(disabled_web_ui.is_err());
    }
}
