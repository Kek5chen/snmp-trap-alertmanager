use crate::sanitize::{
    clean_alert_name, greedy_truncate_labels_prefix, greedy_truncate_labels_suffix,
};
use anyhow::{anyhow, bail};
use log::warn;
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{Column, Row};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::str::FromStr;
use time::{OffsetDateTime, PrimitiveDateTime};

const DROP_COLUMNS: &[&str] = &["mib", "oid", "source", "version", "sysUpTime.0", "host"];

#[derive(Debug, Clone, Eq, Serialize)]
pub struct Alert {
    hash: u64,
    severity: Severity,
    community: String,
    name: String,
    times: Vec<OffsetDateTime>,
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    Critical = 2,
}

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
        .to_string();
        write!(f, "{}", str)
    }
}

impl FromStr for Severity {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const CRITICAL: &[&str] = &["crit", "error", "major", "high"];
        const WARN: &[&str] = &["warn", "minor", "mid"];
        const INFO: &[&str] = &["info", "normal", "debug", "low"];

        let s = s.to_lowercase();
        if CRITICAL.iter().any(|c| s.contains(c)) {
            Ok(Severity::Critical)
        } else if WARN.iter().any(|w| s.contains(w)) {
            Ok(Severity::Warning)
        } else if INFO.iter().any(|i| s.contains(i)) {
            Ok(Severity::Warning)
        } else {
            Err(anyhow!("unknown severity"))
        }
    }
}

impl Alert {
    fn new(
        name: String,
        severity: Severity,
        community: String,
        times: Vec<OffsetDateTime>,
        labels: BTreeMap<String, String>,
    ) -> Alert {
        let mut alert = Alert {
            hash: 0,
            severity,
            community,
            name,
            times,
            labels,
        };

        let mut hasher = DefaultHasher::default();
        Hash::hash(&alert, &mut hasher);
        let hash = hasher.finish();
        alert.hash = hash;

        alert
    }

    pub fn earliest(&self) -> OffsetDateTime {
        self.times
            .iter()
            .min()
            .cloned()
            .unwrap_or_else(|| OffsetDateTime::now_utc())
    }

    pub fn latest(&self) -> OffsetDateTime {
        self.times
            .iter()
            .max()
            .cloned()
            .unwrap_or_else(|| OffsetDateTime::now_utc())
    }

    pub fn pretty_name(&self) -> String {
        clean_alert_name(self.name.clone())
    }

    pub fn raw_name(&self) -> &str {
        &self.name
    }

    pub fn pretty_labels(&self) -> BTreeMap<String, String> {
        let mut labels = self.labels.clone();
        _ = greedy_truncate_labels_prefix(&mut labels);
        _ = greedy_truncate_labels_suffix(&mut labels);
        labels
    }

    pub fn raw_labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }

    pub fn community(&self) -> &str {
        &self.community
    }

    pub fn times(&self) -> &[OffsetDateTime] {
        &self.times
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }
}

impl Hash for Alert {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.severity.hash(state);
        self.community.hash(state);
        self.labels.hash(state);
    }
}

impl PartialEq for Alert {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.severity == other.severity
            && self.labels == other.labels
            && self.community == other.community
    }
}

pub fn map_traps_to_alerts(traps: &[PgRow]) -> HashSet<Alert> {
    let raw_alerts = traps.iter().map(TryInto::try_into).filter_map(|r| match r {
        Ok(alert) => Some(alert),
        Err(e) => {
            warn!("Invalid alert database row: {e}");
            None
        }
    });

    generate_alerts(raw_alerts)
}

impl TryFrom<&PgRow> for Alert {
    type Error = anyhow::Error;

    fn try_from(row: &PgRow) -> Result<Self, Self::Error> {
        let mut name: Option<String> = None;
        let mut labels = BTreeMap::new();
        let mut time: Option<PrimitiveDateTime> = None;
        let mut community: Option<String> = None;

        for col in row.columns() {
            if DROP_COLUMNS.contains(&col.name()) {
                continue;
            }

            match col.name() {
                "time" => time = Some(row.try_get(col.ordinal())?),
                "name" => name = Some(row.try_get(col.ordinal())?),
                "community" => community = Some(row.try_get(col.ordinal())?),
                _ => {
                    if labels.contains_key(col.name()) {
                        continue;
                    }

                    let Some(value) = row.try_get::<'_, Option<String>, _>(col.ordinal())? else {
                        continue; // null value in column means it's a label for a different trap
                    };

                    if value.is_empty() {
                        continue; // empty values are kind of useless
                    }

                    let key = col.name().to_owned();

                    labels.insert(key, value);
                }
            }
        }

        let Some(name) = name else {
            bail!("No name in database row found for alert");
        };

        let Some(community) = community else {
            bail!("No community in database row found for alert");
        };

        let Some(time) = time else {
            bail!("No time in database row found for alert");
        };

        let severity = extract_severity(&mut labels).unwrap_or_else(|| Severity::Critical);
        let time = time.assume_utc();

        Ok(Alert::new(name, severity, community, vec![time], labels))
    }
}

fn extract_severity(labels: &mut BTreeMap<String, String>) -> Option<Severity> {
    const SEVERITY: &[&str] = &["severity"];
    let (k, v) = labels.iter().find(|(k, _)| {
        for severity in SEVERITY {
            if k.to_lowercase().contains(severity) {
                return true;
            }
        }
        false
    })?;

    let Ok(severity) = Severity::from_str(v) else {
        warn!("Failed to match up severity. Found {k:?}, but {v:?} was not a valid severity.");
        return None;
    };

    _ = labels.remove(&k.clone());

    Some(severity)
}

fn generate_alerts(raw_alerts: impl IntoIterator<Item = Alert>) -> HashSet<Alert> {
    let mut alerts = HashSet::new();

    for alert in raw_alerts {
        let entry = alerts.take(&alert);
        match entry {
            None => alerts.insert(alert),
            Some(mut existing) => {
                existing.times.extend(alert.times);
                alerts.insert(existing)
            }
        };
    }

    alerts
}
