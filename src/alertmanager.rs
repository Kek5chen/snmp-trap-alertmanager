use crate::alerts::Alert;
use crate::config::CONFIG;
use crate::trap_db::TrapDb;
use log::{debug, warn};
use reqwest::Client;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub struct AlertmanagerRelay {
    url: String,
    client: Client,
    db: Arc<TrapDb>,
    last_announce_try: Instant,
}

impl AlertmanagerRelay {
    pub fn new(url: String, db: Arc<TrapDb>) -> Self {
        Self {
            url,
            client: Client::default(),
            db,
            last_announce_try: Instant::now() - Duration::days(360),
        }
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
                    warn!("Couldn't relay alerts to alertmanager: {e}");
                }
            }

            self.last_announce_try = Instant::now()
        }
    }

    pub async fn relay_alerts(&self) -> anyhow::Result<()> {
        let alerts = self.db.cached_alerts().await;
        let alerts_data = self.alerts_to_alertmanager(&*alerts);
        drop(alerts);

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
            .map(AlertmanagerAlert::from_alert)
            .collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertmanagerAlert {
    #[serde(rename = "startsAt")]
    starts_at: String,
    #[serde(rename = "endsAt")]
    ends_at: String,
    labels: BTreeMap<String, String>,
    #[serde(rename = "generatorURL")]
    generator_url: String,
}

impl AlertmanagerAlert {
    fn from_alert(alert: &Alert) -> Self {
        let starts_at: OffsetDateTime = alert.earliest();
        let ends_at: OffsetDateTime =
            OffsetDateTime::now_utc() + CONFIG.alertmanager_announce_duration() * 3;

        let mut labels = alert.pretty_labels();
        labels.insert("alertname".to_string(), alert.pretty_name());
        labels.insert("severity".to_string(), alert.severity().to_string());
        labels.insert(
            CONFIG.alertmanager_community_label().to_string(),
            alert.community().to_string(),
        );

        AlertmanagerAlert {
            starts_at: starts_at.format(&Rfc3339).unwrap(),
            ends_at: ends_at.format(&Rfc3339).unwrap(),
            labels,
            generator_url: CONFIG.web_url().to_string(),
        }
    }
}
