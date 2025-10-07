use crate::alerts::{Alert, Severity};
use crate::config::CONFIG;
use crate::enrichment::AlertEnrichment;
use crate::trap_db::TrapDb;
use log::{debug, info, warn};
use reqwest::Client;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use itertools::Itertools;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub struct AlertmanagerRelay {
    url: String,
    client: Client,
    db: Arc<TrapDb>,
    last_announce_try: Instant,
    enrichment: AlertEnrichment,
}

impl AlertmanagerRelay {
    pub fn new(url: String, db: Arc<TrapDb>) -> anyhow::Result<Self> {
        let mut enrichment = AlertEnrichment::new();
        if let Some(alert_dir) = CONFIG.alert_dir() {
            enrichment.load_directory(alert_dir)?;
        }

        info!("Loaded {} alert enrichments", enrichment.count());

        Ok(Self {
            url,
            client: Client::default(),
            db,
            last_announce_try: Instant::now() - Duration::days(360),
            enrichment,
        })
    }

    pub async fn run_relay_blocking(&mut self) {
        loop {
            let next_announce = self.last_announce_try + CONFIG.alertmanager_announce_duration();
            tokio::time::sleep_until(next_announce.into()).await;

            match self.relay_alerts().await {
                Ok(_) => {
                    debug!("SNMP Trap alerts successfully relayed to Alertmanager");
                }
                Err(e) => {
                    warn!("Couldn't relay alerts to alertmanager: {e:?}");
                }
            }

            self.last_announce_try = Instant::now()
        }
    }

    pub async fn relay_alerts(&self) -> anyhow::Result<()> {
        let alerts = self.db.cached_alerts().await;
        let mut alerts_data = self.alerts_to_alertmanager(&*alerts);
        drop(alerts);
        self.enrich(&mut alerts_data)?;

        self.client
            .post(format!("{}/api/v2/alerts", self.url))
            .json(&alerts_data)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    fn alerts_to_alertmanager<'a>(
        &self,
        alerts: impl IntoIterator<Item = &'a Alert>,
    ) -> Vec<AlertmanagerAlert> {
        alerts
            .into_iter()
            .map(AlertmanagerAlert::from)
            .collect_vec()
    }

    fn enrich(&self, alerts: &mut [AlertmanagerAlert]) -> anyhow::Result<()> {
        for alert in alerts.iter_mut() {
            alert.enrich(&self.enrichment)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertmanagerAlert {
    #[serde(rename = "startsAt")]
    starts_at: String,
    #[serde(rename = "endsAt")]
    ends_at: String,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
    #[serde(rename = "generatorURL")]
    generator_url: String,
}

impl AlertmanagerAlert {
    pub fn new(
        starts_at: OffsetDateTime,
        ends_at: OffsetDateTime,
        name: impl Into<String>,
        community: impl Into<String>,
        severity: Severity,
        labels: Option<BTreeMap<String, String>>,
        annotations: Option<BTreeMap<String, String>>,
    ) -> Self {
        let mut labels = labels.unwrap_or_default();
        labels.insert("alertname".to_string(), name.into());
        labels.insert("severity".to_string(), severity.to_string());
        labels.insert(
            CONFIG.alertmanager_community_label().to_string(),
            community.into(),
        );

        AlertmanagerAlert {
            starts_at: starts_at.format(&Rfc3339).unwrap(),
            ends_at: ends_at.format(&Rfc3339).unwrap(),
            labels,
            annotations: annotations.unwrap_or_default(),
            generator_url: CONFIG.web_url().to_string(),
        }
    }

    pub fn enrich(&mut self, enrichment: &AlertEnrichment) -> anyhow::Result<()> {
        enrichment.apply_all(self)
    }

    pub fn name(&self) -> &str {
        debug_assert!(self.labels.contains_key("alertname"));
        self.labels
            .get("alertname")
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn labels(&self) -> &BTreeMap<String, String> {
        debug_assert!(self.labels.contains_key("alertname"));
        debug_assert!(self.labels.contains_key("severity"));
        debug_assert!(
            self.labels
                .contains_key(CONFIG.alertmanager_community_label())
        );

        &self.labels
    }

    pub fn is_restricted_label(name: &str) -> bool {
        name == "alertname" || name == "severity" || name == CONFIG.alertmanager_community_label()
    }

    pub fn add_label(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        if Self::is_restricted_label(&name) {
            return;
        }
        self.labels.insert(name, value.into());
    }

    pub fn add_labels<'a, L, S, S2>(&mut self, labels: L)
    where
        L: IntoIterator<Item = (S, S2)>,
        S: Into<String> + 'a,
        S2: Into<String> + 'a,
    {
        for (n, v) in labels {
            self.add_label(n, v);
        }
    }

    pub fn remove_label(&mut self, name: &str) -> Option<String> {
        if Self::is_restricted_label(name) {
            return None;
        }
        self.labels.remove(name)
    }

    pub fn add_annotation(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.annotations.insert(name.into(), value.into());
    }

    pub fn add_annotations<'a, L, S, S2>(&mut self, labels: L)
    where
        L: IntoIterator<Item = (S, S2)>,
        S: Into<String> + 'a,
        S2: Into<String> + 'a,
    {
        for (n, v) in labels {
            self.add_annotation(n, v);
        }
    }
}

impl From<&Alert> for AlertmanagerAlert {
    fn from(alert: &Alert) -> Self {
        let starts_at: OffsetDateTime = alert.earliest();
        let ends_at: OffsetDateTime =
            OffsetDateTime::now_utc() + CONFIG.alertmanager_announce_duration() * 3;

        let labels = alert.pretty_labels();

        AlertmanagerAlert::new(
            starts_at,
            ends_at,
            alert.pretty_name(),
            alert.community(),
            alert.severity(),
            Some(labels),
            None
        )
    }
}
