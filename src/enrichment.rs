use crate::alertmanager::AlertmanagerAlert;
use itertools::Itertools;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tera::{Context, Tera};

pub struct AlertEnrichment {
    definitions: Vec<AlertEnrichmentDefinition>,
}

impl AlertEnrichment {
    pub fn new() -> Self {
        AlertEnrichment {
            definitions: Vec::new(),
        }
    }

    pub fn load_directory(&mut self, dir: &Path) -> anyhow::Result<usize> {
        let amount = self.count();
        for entry in dir.read_dir()? {
            let file = AlertEnrichmentFile::load(&entry?.path())?;
            let alerts: Vec<_> = file
                .alerts
                .into_iter()
                .map(|a| a.try_into())
                .try_collect()?;
            self.definitions.extend(alerts);
        }
        Ok(self.count() - amount)
    }

    pub fn apply_all(&self, alert: &mut AlertmanagerAlert) -> anyhow::Result<()> {
        for definition in &self.definitions {
            definition.apply(alert)?;
        }
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.definitions.len()
    }
}

#[derive(Debug, Deserialize)]
pub struct AlertEnrichmentFile {
    alerts: Vec<RawAlertEnrichmentDefinition>,
}

impl AlertEnrichmentFile {
    pub fn load(file: &Path) -> anyhow::Result<AlertEnrichmentFile> {
        let content = fs::read_to_string(file)?;
        Ok(serde_norway::from_str(&content)?)
    }
}

#[derive(Debug, Deserialize)]
pub struct RawAlertEnrichmentDefinition {
    #[serde(with = "serde_regex")]
    name: regex::Regex,
    labels: Option<HashMap<String, String>>,
    annotations: Option<HashMap<String, String>>,
    #[serde(with = "serde_regex")]
    drop_labels: Option<Vec<regex::Regex>>,
}

pub struct AlertEnrichmentDefinition {
    name: regex::Regex,
    label_templates: Tera,
    annotation_templates: Tera,
    drop_labels: Vec<regex::Regex>,
}

impl TryFrom<RawAlertEnrichmentDefinition> for AlertEnrichmentDefinition {
    type Error = anyhow::Error;

    fn try_from(raw: RawAlertEnrichmentDefinition) -> Result<Self, Self::Error> {
        Self::new(raw.name, raw.labels, raw.annotations, raw.drop_labels)
    }
}

impl AlertEnrichmentDefinition {
    pub fn new(
        name: regex::Regex,
        annotations: Option<HashMap<String, String>>,
        labels: Option<HashMap<String, String>>,
        drop_labels: Option<Vec<regex::Regex>>,
    ) -> anyhow::Result<Self> {
        let annotations = annotations.unwrap_or_default();
        let labels = labels.unwrap_or_default();
        let drop_labels = drop_labels.unwrap_or_default();

        let label_templates = build_templates(&labels)?;
        let annotation_templates = build_templates(&annotations)?;

        Ok(AlertEnrichmentDefinition {
            name,
            label_templates,
            annotation_templates,
            drop_labels,
        })
    }

    pub fn applies_to(&self, alert: &AlertmanagerAlert) -> bool {
        self.name
            .find_at(alert.name(), 0)
            .is_some_and(|m| m.len() == alert.name().len())
    }

    pub fn apply(&self, alert: &mut AlertmanagerAlert) -> anyhow::Result<bool> {
        if !self.applies_to(alert) {
            return Ok(false);
        }

        alert.add_labels(&generate_labels(&self.label_templates, alert)?);
        alert.add_annotations(&generate_labels(&self.annotation_templates, alert)?);

        let label_names = alert.labels().keys().cloned().collect_vec();
        for rgx in &self.drop_labels {
            for name in &label_names {
                if rgx.find_at(name, 0).is_some_and(|m| m.len() == name.len()) {
                    alert.remove_label(name);
                    break;
                }
            }
        }

        Ok(true)
    }
}

fn build_templates<I, S, S2>(values: I) -> tera::Result<Tera>
where
    I: IntoIterator<Item = (S, S2)>,
    S: AsRef<str>,
    S2: AsRef<str>,
{
    let mut tera = Tera::default();
    tera.set_strict(false);
    for (k, v) in values {
        tera.add_raw_template(k.as_ref(), v.as_ref())?;
    }
    Ok(tera)
}

fn build_context(alert: &AlertmanagerAlert) -> tera::Result<Context> {
    let labels = alert.labels();
    Context::from_value(json!({
        "labels": labels,
    }))
}

pub fn generate_labels(
    templates: &Tera,
    alert: &AlertmanagerAlert,
) -> tera::Result<HashMap<String, String>> {
    let mut labels = HashMap::new();
    let ctx = build_context(alert)?;
    for name in templates.get_template_names() {
        let value = templates.render(name, &ctx)?;
        labels.insert(name.to_string(), value);
    }
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use crate::alertmanager::AlertmanagerAlert;
    use crate::alerts::Severity;
    use crate::enrichment::AlertEnrichmentDefinition;
    use regex::Regex;
    use time::OffsetDateTime;

    #[test]
    fn enrichment_applies() {
        let def = AlertEnrichmentDefinition::new(Regex::new(r"test.*").unwrap(), None, None, None)
            .unwrap();
        let alert = AlertmanagerAlert::new(
            OffsetDateTime::now_utc(),
            OffsetDateTime::now_utc(),
            "testAlert",
            "somejob",
            Severity::Info,
            None,
            None,
        );

        assert!(def.applies_to(&alert));
    }
}
