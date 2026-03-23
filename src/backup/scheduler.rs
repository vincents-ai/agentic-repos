use crate::backup::models::{BackupSchedule, BackupType};
use crate::backup::service::BackupService;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub name: String,
    pub backup_type: BackupType,
    pub tenant_id: Option<String>,
    pub repository_ids: Vec<String>,
    pub include_lfs: bool,
    pub include_database: bool,
    pub include_config: bool,
    pub retention_days: u32,
    pub cron_expression: String,
    pub enabled: bool,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<BackupSchedule> for ScheduledJob {
    fn from(schedule: BackupSchedule) -> Self {
        Self {
            name: schedule.name,
            backup_type: schedule.backup_type,
            tenant_id: schedule.tenant_id,
            repository_ids: schedule.repository_ids,
            include_lfs: schedule.include_lfs,
            include_database: schedule.include_database,
            include_config: schedule.include_config,
            retention_days: schedule.retention_days,
            cron_expression: schedule.cron_expression,
            enabled: schedule.enabled,
            last_run: None,
            next_run: None,
        }
    }
}

pub struct BackupScheduler {
    backup_service: Arc<BackupService>,
    jobs: Arc<RwLock<HashMap<String, ScheduledJob>>>,
    running: Arc<RwLock<bool>>,
}

impl BackupScheduler {
    pub fn new(backup_service: Arc<BackupService>) -> Self {
        Self {
            backup_service,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn add_schedule(&self, schedule: BackupSchedule) -> Result<(), String> {
        let name = schedule.name.clone();
        let mut job = ScheduledJob::from(schedule);
        job.next_run = self.calculate_next_run(&job.cron_expression)?;

        let mut jobs = self.jobs.write().await;
        jobs.insert(name.clone(), job);

        info!("Added backup schedule: {}", name);
        Ok(())
    }

    pub async fn remove_schedule(&self, name: &str) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        
        if jobs.remove(name).is_some() {
            info!("Removed backup schedule: {}", name);
            Ok(())
        } else {
            Err(format!("Schedule not found: {}", name))
        }
    }

    pub async fn enable_schedule(&self, name: &str) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        
        if let Some(job) = jobs.get_mut(name) {
            job.enabled = true;
            job.next_run = self.calculate_next_run(&job.cron_expression)?;
            info!("Enabled backup schedule: {}", name);
            Ok(())
        } else {
            Err(format!("Schedule not found: {}", name))
        }
    }

    pub async fn disable_schedule(&self, name: &str) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        
        if let Some(job) = jobs.get_mut(name) {
            job.enabled = false;
            job.next_run = None;
            info!("Disabled backup schedule: {}", name);
            Ok(())
        } else {
            Err(format!("Schedule not found: {}", name))
        }
    }

    pub async fn list_schedules(&self) -> Vec<ScheduledJob> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    pub async fn get_schedule(&self, name: &str) -> Option<ScheduledJob> {
        let jobs = self.jobs.read().await;
        jobs.get(name).cloned()
    }

    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            warn!("Backup scheduler is already running");
            return;
        }
        *running = true;
        drop(running);

        info!("Starting backup scheduler");

        let jobs = Arc::clone(&self.jobs);
        let backup_service = Arc::clone(&self.backup_service);
        let running_flag = Arc::clone(&self.running);

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                let is_running = *running_flag.read().await;
                if !is_running {
                    break;
                }

                let jobs_snapshot: Vec<ScheduledJob> = {
                    let jobs_guard = jobs.read().await;
                    jobs_guard.values().cloned().collect()
                };

                for mut job in jobs_snapshot {
                    if !job.enabled {
                        continue;
                    }

                    if let Some(next_run) = job.next_run {
                        if next_run <= chrono::Utc::now() {
                            debug!("Executing scheduled backup: {}", job.name);
                            
                            let tenant_id = job.tenant_id.clone().unwrap_or_else(|| "default".to_string());
                            
                            match backup_service
                                .create_backup(
                                    job.backup_type.clone(),
                                    tenant_id,
                                    job.repository_ids.clone(),
                                    job.include_lfs,
                                    job.include_database,
                                    job.include_config,
                                    Some(job.retention_days),
                                )
                                .await
                            {
                                Ok(backup) => {
                                    let backup_id = backup.id;
                                    let service = Arc::clone(&backup_service);
                                    
                                    tokio::spawn(async move {
                                        if let Err(e) = service.execute_backup(backup_id).await {
                                            error!("Scheduled backup {} failed: {}", backup_id, e);
                                        }
                                    });

                                    job.last_run = Some(chrono::Utc::now());
                                    
                                    if let Ok(next) = BackupScheduler::calculate_next_run_static(&job.cron_expression) {
                                        job.next_run = Some(next);
                                    }

                                    let mut jobs_guard = jobs.write().await;
                                    if let Some(stored_job) = jobs_guard.get_mut(&job.name) {
                                        stored_job.last_run = job.last_run;
                                        stored_job.next_run = job.next_run;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to create scheduled backup: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            info!("Backup scheduler stopped");
        });
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopping backup scheduler");
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    fn calculate_next_run(&self, cron_expression: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
        Self::calculate_next_run_static(cron_expression)
    }

    fn calculate_next_run_static(cron_expression: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
        let parts: Vec<&str> = cron_expression.split_whitespace().collect();
        
        if parts.len() != 5 {
            return Err(format!("Invalid cron expression: {}", cron_expression));
        }

        let now = chrono::Utc::now();
        let next = now + chrono::Duration::days(1);
        
        debug!("Calculated next run for '{}' as approximately {}", cron_expression, next);
        
        Ok(Some(next))
    }

    pub async fn trigger_schedule(&self, name: &str) -> Result<uuid::Uuid, String> {
        let jobs = self.jobs.read().await;
        
        let job = jobs
            .get(name)
            .ok_or_else(|| format!("Schedule not found: {}", name))?;

        if !job.enabled {
            return Err(format!("Schedule {} is disabled", name));
        }

        let tenant_id = job.tenant_id.clone().unwrap_or_else(|| "default".to_string());

        let backup = self
            .backup_service
            .create_backup(
                job.backup_type.clone(),
                tenant_id,
                job.repository_ids.clone(),
                job.include_lfs,
                job.include_database,
                job.include_config,
                Some(job.retention_days),
            )
            .await
            .map_err(|e| e.to_string())?;

        let backup_id = backup.id;
        let service = Arc::clone(&self.backup_service);

        tokio::spawn(async move {
            if let Err(e) = service.execute_backup(backup_id).await {
                error!("Triggered backup {} failed: {}", backup_id, e);
            }
        });

        info!("Triggered backup schedule: {}", name);
        Ok(backup_id)
    }

    pub async fn load_schedules(&self, schedules: Vec<BackupSchedule>) {
        for schedule in schedules {
            if let Err(e) = self.add_schedule(schedule).await {
                error!("Failed to load schedule: {}", e);
            }
        }
    }
}
