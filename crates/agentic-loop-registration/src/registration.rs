//! Registration manager — handles the full registration lifecycle.

use crate::error::RegistrationError;
use crate::secret::SecretGenerator;
use adp_proto::rbac::ServiceType;
use adp_proto::registration::{InstanceCapabilities, RegistrationInfo, ServiceMetadata};
use chrono::{Duration, Utc};
use std::sync::RwLock;
use tracing::{debug, info, instrument};

/// Status of a service registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationStatus {
    /// Not registered.
    Unregistered,
    /// Registered and active.
    Active,
    /// Registration expired.
    Expired,
    /// Pending renewal.
    PendingRenewal,
}

/// Configuration for the registration manager.
#[derive(Debug, Clone)]
pub struct RegistrationConfig {
    /// Default TTL for new registrations in seconds.
    pub default_ttl_secs: i64,
    /// Threshold at which to trigger renewal (seconds remaining).
    pub renewal_threshold_secs: i64,
    /// Maximum TTL allowed (seconds).
    pub max_ttl_secs: i64,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            default_ttl_secs: 3600,      // 1 hour
            renewal_threshold_secs: 300,  // 5 minutes
            max_ttl_secs: 86400,          // 24 hours
        }
    }
}

/// Manages service registration for an agentic-loop instance.
pub struct RegistrationManager {
    config: RegistrationConfig,
    secret_gen: SecretGenerator,
    registration: RwLock<Option<RegistrationInfo>>,
}

impl RegistrationManager {
    /// Create a new registration manager.
    pub fn new(config: RegistrationConfig) -> Self {
        Self {
            config,
            secret_gen: SecretGenerator::new(),
            registration: RwLock::new(None),
        }
    }

    /// Register a new service instance.
    ///
    /// Generates a secret, creates the registration info, and returns it
    /// for storage in git refs and SSE connection.
    #[instrument(skip(self))]
    pub fn register(
        &self,
        service_type: ServiceType,
        capabilities: Vec<InstanceCapabilities>,
        project_ids: Vec<String>,
    ) -> Result<RegistrationInfo, RegistrationError> {
        // Check not already registered
        {
            let reg = self.registration.read().unwrap();
            if let Some(existing) = reg.as_ref() {
                if !existing.is_expired() {
                    return Err(RegistrationError::AlreadyRegistered(
                        existing.instance_id.to_string(),
                    ));
                }
            }
        }

        let instance_id = SecretGenerator::generate_instance_id();
        let secret = self.secret_gen.generate()?;
        let now = Utc::now();
        let expires_at = now + Duration::seconds(self.config.default_ttl_secs);

        let info = RegistrationInfo {
            instance_id,
            secret,
            created_at: now,
            expires_at,
            service_type,
            metadata: ServiceMetadata::detect(),
            capabilities,
            project_ids,
        };

        info!(
            instance_id = %info.instance_id,
            service_type = %service_type,
            ttl = self.config.default_ttl_secs,
            "Service registered"
        );

        *self.registration.write().unwrap() = Some(info.clone());
        Ok(info)
    }

    /// Get the current registration, if any.
    pub fn current(&self) -> Option<RegistrationInfo> {
        self.registration.read().unwrap().clone()
    }

    /// Get the current registration status.
    pub fn status(&self) -> RegistrationStatus {
        let reg = self.registration.read().unwrap();
        match reg.as_ref() {
            None => RegistrationStatus::Unregistered,
            Some(info) => {
                if info.is_expired() {
                    RegistrationStatus::Expired
                } else if info.needs_renewal(self.config.renewal_threshold_secs) {
                    RegistrationStatus::PendingRenewal
                } else {
                    RegistrationStatus::Active
                }
            }
        }
    }

    /// Renew the registration secret.
    ///
    /// Generates a new secret and extends the TTL.
    #[instrument(skip(self))]
    pub fn renew(&self) -> Result<RegistrationInfo, RegistrationError> {
        let mut reg = self.registration.write().unwrap();
        let info = reg
            .as_mut()
            .ok_or(RegistrationError::NotRegistered)?;

        if info.is_expired() {
            return Err(RegistrationError::Expired {
                expires_at: info.expires_at.to_rfc3339(),
            });
        }

        // Generate new secret
        let new_secret = self.secret_gen.generate()?;
        info.secret = new_secret;

        // Extend TTL
        let now = Utc::now();
        info.expires_at = now + Duration::seconds(self.config.default_ttl_secs);

        debug!(
            instance_id = %info.instance_id,
            new_expires_at = %info.expires_at,
            "Registration renewed"
        );

        Ok(info.clone())
    }

    /// Renew with a custom TTL.
    pub fn renew_with_ttl(&self, ttl_secs: i64) -> Result<RegistrationInfo, RegistrationError> {
        if ttl_secs > self.config.max_ttl_secs {
            return Err(RegistrationError::RenewalFailed(format!(
                "TTL {} exceeds maximum {}",
                ttl_secs, self.config.max_ttl_secs
            )));
        }

        let mut reg = self.registration.write().unwrap();
        let info = reg
            .as_mut()
            .ok_or(RegistrationError::NotRegistered)?;

        if info.is_expired() {
            return Err(RegistrationError::Expired {
                expires_at: info.expires_at.to_rfc3339(),
            });
        }

        let new_secret = self.secret_gen.generate()?;
        info.secret = new_secret;
        info.expires_at = Utc::now() + Duration::seconds(ttl_secs);

        Ok(info.clone())
    }

    /// Deregister the instance.
    #[instrument(skip(self))]
    pub fn deregister(&self) -> Result<(), RegistrationError> {
        let mut reg = self.registration.write().unwrap();
        if reg.is_none() {
            return Err(RegistrationError::NotRegistered);
        }
        *reg = None;
        info!("Instance deregistered");
        Ok(())
    }

    /// Check if renewal is needed.
    pub fn needs_renewal(&self) -> bool {
        matches!(self.status(), RegistrationStatus::PendingRenewal)
    }

    /// Get the current secret.
    pub fn secret(&self) -> Option<String> {
        self.registration
            .read()
            .unwrap()
            .as_ref()
            .map(|i| i.secret.clone())
    }

    /// Get TTL remaining in seconds.
    pub fn ttl_remaining(&self) -> Option<i64> {
        self.registration
            .read()
            .unwrap()
            .as_ref()
            .map(|i| i.ttl_remaining())
    }
}

impl Default for RegistrationManager {
    fn default() -> Self {
        Self::new(RegistrationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mgr = RegistrationManager::default();
        let info = mgr
            .register(
                ServiceType::AgenticLoop,
                vec![InstanceCapabilities::Workflows],
                vec!["default".to_string()],
            )
            .unwrap();

        assert!(!info.instance_id.to_string().is_empty());
        assert!(!info.secret.is_empty());
        assert_eq!(info.service_type, ServiceType::AgenticLoop);
        assert_eq!(mgr.status(), RegistrationStatus::Active);
    }

    #[test]
    fn register_prevents_double() {
        let mgr = RegistrationManager::default();
        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();
        let result = mgr.register(ServiceType::AgenticLoop, vec![], vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn renew_updates_secret_and_ttl() {
        let mgr = RegistrationManager::default();
        let original = mgr
            .register(ServiceType::CiRunner, vec![], vec![])
            .unwrap();
        let original_secret = original.secret.clone();

        let renewed = mgr.renew().unwrap();
        assert_ne!(renewed.secret, original_secret);
        assert!(renewed.expires_at > original.expires_at);
    }

    #[test]
    fn renew_with_ttl_respects_max() {
        let config = RegistrationConfig {
            max_ttl_secs: 7200,
            ..Default::default()
        };
        let mgr = RegistrationManager::new(config);
        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();

        let result = mgr.renew_with_ttl(86400);
        assert!(result.is_err());
    }

    #[test]
    fn deregister_clears() {
        let mgr = RegistrationManager::default();
        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();
        mgr.deregister().unwrap();
        assert_eq!(mgr.status(), RegistrationStatus::Unregistered);
        assert!(mgr.current().is_none());
    }

    #[test]
    fn deregister_not_registered_fails() {
        let mgr = RegistrationManager::default();
        assert!(mgr.deregister().is_err());
    }

    #[test]
    fn ttl_remaining() {
        let mgr = RegistrationManager::default();
        assert!(mgr.ttl_remaining().is_none());

        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();
        let ttl = mgr.ttl_remaining().unwrap();
        assert!(ttl > 3500 && ttl <= 3600);
    }

    #[test]
    fn secret_available() {
        let mgr = RegistrationManager::default();
        assert!(mgr.secret().is_none());

        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();
        assert!(mgr.secret().is_some());
    }

    #[test]
    fn needs_renewal_when_close_to_expiry() {
        let config = RegistrationConfig {
            default_ttl_secs: 200, // Below 300 threshold
            renewal_threshold_secs: 300,
            ..Default::default()
        };
        let mgr = RegistrationManager::new(config);
        mgr.register(ServiceType::AgenticLoop, vec![], vec![])
            .unwrap();

        // TTL is 200, threshold is 300, so it should need renewal immediately
        assert!(mgr.needs_renewal());
        assert_eq!(mgr.status(), RegistrationStatus::PendingRenewal);
    }
}
