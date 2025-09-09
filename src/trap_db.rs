use crate::alerts::{Alert, map_traps_to_alerts};
use log::{error, warn};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio::time::Instant;

#[derive(Clone)]
pub struct TrapDb {
    pool: PgPool,
    cached_alerts: Arc<RwLock<HashSet<Alert>>>,
    last_update: Arc<RwLock<Instant>>,
}

impl TrapDb {
    pub fn new(conn_url: &str) -> anyhow::Result<TrapDb> {
        let pool = PgPool::connect_lazy(conn_url)?;

        Ok(TrapDb {
            pool,
            cached_alerts: Arc::default(),
            last_update: Arc::new(RwLock::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(99999))
                    .expect("Instant should not overflow"),
            )),
        })
    }

    pub async fn cached_alerts<'a>(&'a self) -> RwLockReadGuard<'a, HashSet<Alert>> {
        if self.last_update.read().await.elapsed() > Duration::from_secs(5) {
            self.update_cache().await;
        }

        self.cached_alerts.read().await
    }

    pub async fn update_cache(&self) {
        match self.fetch_alerts().await {
            Err(e) => error!("Error fetching alerts: {}", e),
            Ok(alerts) => {
                *self.cached_alerts.write().await = alerts;
                *self.last_update.write().await = Instant::now();
            }
        }
    }

    pub async fn fetch_raw_traps(&self) -> anyhow::Result<Vec<PgRow>> {
        let traps = sqlx::query(
            r#"
        SELECT * FROM "snmp_trap"
    "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(traps)
    }

    pub async fn fetch_alerts(&self) -> anyhow::Result<HashSet<Alert>> {
        let traps = self.fetch_raw_traps().await?;
        Ok(map_traps_to_alerts(&traps))
    }

    pub async fn clear_alerts(&self, hash: u64) -> anyhow::Result<()> {
        let alerts = self.cached_alerts().await.clone();

        let Some(alert) = alerts.iter().find(|a| a.hash() == hash) else {
            warn!("Alert lookup by hash supplied no results. Already deleted?");
            return Ok(());
        };

        self.delete_alert(alert).await?;
        self.update_cache().await;

        Ok(())
    }

    pub async fn delete_alert(&self, alert: &Alert) -> anyhow::Result<()> {
        make_label_query(alert).build().execute(&self.pool).await?;

        Ok(())
    }
}

fn make_label_query(alert: &'_ Alert) -> QueryBuilder<'_, Postgres> {
    let mut builder = QueryBuilder::new("DELETE FROM snmp_trap WHERE name = ");

    builder.push_bind(alert.raw_name());
    builder.push(r#" AND community = "#);
    builder.push_bind(alert.community());

    for label in alert.raw_labels().iter() {
        if label.0.contains('"') {
            error!(
                "Label {:?} contains unquoted string in alert {}. Since the label key is used as the database field, this shouldn't happen. Skipping.",
                label.0,
                alert.raw_name()
            );
            continue;
        }

        builder.push(r#" AND ""#);
        builder.push(label.0);
        builder.push(r#"" = "#);
        builder.push_bind(label.1);

        println!("{} = {}", label.0, label.1);
    }

    builder
}
