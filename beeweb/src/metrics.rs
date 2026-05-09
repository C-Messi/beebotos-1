//! Prometheus metrics for BeeWeb Update Server

use std::sync::Arc;

use prometheus::{
    Counter, GaugeVec, Histogram, HistogramOpts, IntCounter, IntCounterVec, Registry,
};

/// Update metrics collection
#[derive(Debug, Clone)]
pub struct UpdateMetrics {
    registry: Arc<Registry>,
    /// Total number of update checks
    pub update_check_total: IntCounter,
    /// Total number of available updates detected
    pub update_available_total: IntCounterVec,
    /// Total download bytes
    pub update_download_bytes_total: Counter,
    /// Download duration histogram
    pub update_download_duration_seconds: Histogram,
    /// Install duration histogram
    pub update_install_duration_seconds: Histogram,
    /// Total successful updates
    pub update_success_total: IntCounter,
    /// Total failed updates by error type
    pub update_failure_total: IntCounterVec,
    /// Total rollbacks
    pub update_rollback_total: IntCounter,
    /// Current version gauge (by app)
    pub update_current_version: GaugeVec,
}

impl UpdateMetrics {
    pub fn new() -> anyhow::Result<Self> {
        let registry = Arc::new(Registry::new());

        let update_check_total = IntCounter::new(
            "update_check_total",
            "Total number of version check requests",
        )?;
        registry.register(Box::new(update_check_total.clone()))?;

        let update_available_total = IntCounterVec::new(
            prometheus::Opts::new(
                "update_available_total",
                "Total number of available updates detected",
            ),
            &["app_name", "channel"],
        )?;
        registry.register(Box::new(update_available_total.clone()))?;

        let update_download_bytes_total =
            Counter::new("update_download_bytes_total", "Total bytes downloaded")?;
        registry.register(Box::new(update_download_bytes_total.clone()))?;

        let update_download_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "update_download_duration_seconds",
            "Download duration in seconds",
        ))?;
        registry.register(Box::new(update_download_duration_seconds.clone()))?;

        let update_install_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "update_install_duration_seconds",
            "Install duration in seconds",
        ))?;
        registry.register(Box::new(update_install_duration_seconds.clone()))?;

        let update_success_total =
            IntCounter::new("update_success_total", "Total number of successful updates")?;
        registry.register(Box::new(update_success_total.clone()))?;

        let update_failure_total = IntCounterVec::new(
            prometheus::Opts::new("update_failure_total", "Total number of failed updates"),
            &["app_name", "error_type"],
        )?;
        registry.register(Box::new(update_failure_total.clone()))?;

        let update_rollback_total =
            IntCounter::new("update_rollback_total", "Total number of rollbacks")?;
        registry.register(Box::new(update_rollback_total.clone()))?;

        let update_current_version = GaugeVec::new(
            prometheus::Opts::new(
                "update_current_version",
                "Current reported version by app (value is patch number for tracking)",
            ),
            &["app_name", "device_id", "version"],
        )?;
        registry.register(Box::new(update_current_version.clone()))?;

        Ok(Self {
            registry,
            update_check_total,
            update_available_total,
            update_download_bytes_total,
            update_download_duration_seconds,
            update_install_duration_seconds,
            update_success_total,
            update_failure_total,
            update_rollback_total,
            update_current_version,
        })
    }

    /// Get metrics registry for scraping
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Record a successful check with available update
    pub fn record_available_update(&self, app_name: &str, channel: &str) {
        self.update_available_total
            .with_label_values(&[app_name, channel])
            .inc();
    }

    /// Record a download event
    pub fn record_download(&self, bytes: f64, duration_secs: f64) {
        self.update_download_bytes_total.inc_by(bytes);
        self.update_download_duration_seconds.observe(duration_secs);
    }

    /// Record a successful installation
    pub fn record_success(&self, _app_name: &str, duration_secs: f64) {
        self.update_success_total.inc();
        self.update_install_duration_seconds.observe(duration_secs);
    }

    /// Record a failure
    pub fn record_failure(&self, app_name: &str, error_type: &str) {
        self.update_failure_total
            .with_label_values(&[app_name, error_type])
            .inc();
    }

    /// Record a rollback
    pub fn record_rollback(&self) {
        self.update_rollback_total.inc();
    }

    /// Record current version gauge
    pub fn record_version(&self, app_name: &str, device_id: &str, version: &str, patch: f64) {
        self.update_current_version
            .with_label_values(&[app_name, device_id, version])
            .set(patch);
    }
}

impl Default for UpdateMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics")
    }
}
