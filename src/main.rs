mod alertmanager;
pub mod alerts;
pub mod config;
mod enrichment;
pub mod sanitize;
pub mod trap_db;
pub mod web;

use crate::alertmanager::AlertmanagerRelay;
use crate::config::{CLI, CONFIG};
use crate::enrichment::AlertEnrichment;
use crate::trap_db::TrapDb;
use crate::web::{alerts_view, clear_alert};
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use log::{error, info};
use std::sync::Arc;
use tera::Tera;

#[tokio::main]
async fn main() {
    _ = dotenvy::dotenv();
    env_logger::init();

    if CLI.test_alerts {
        let mut enrichment = AlertEnrichment::new();
        match enrichment.load_directory(CONFIG.alert_dir().unwrap()) {
            Ok(a) => info!("Alert directory loaded. Found {a} definitions for enrichment"),
            Err(e) => error!("Error loading alert directory: {e}"),
        }
        return;
    }

    let db = TrapDb::new(CONFIG.db_url()).unwrap();

    let mut tera = Tera::default();
    tera.add_raw_template("alerts_view", include_str!("../templates/alerts.html"))
        .expect("Failed to add built-in alert template");

    let shared_db = Arc::new(db);
    let shared_tera = Arc::new(tera);

    if let Err(e) = start_relay_thread(shared_db.clone()) {
        error!("Error when configuring alertmanager relay: {e}");
        return;
    }
    run_web_frontend(shared_db.into(), shared_tera.into()).await;
}

async fn run_web_frontend(shared_db: Data<TrapDb>, shared_tera: Data<Tera>) {
    HttpServer::new(move || {
        App::new()
            .app_data(shared_db.clone())
            .app_data(shared_tera.clone())
            .service(alerts_view)
            .service(clear_alert)
    })
    .bind(CONFIG.web_listen())
    .unwrap()
    .run()
    .await
    .unwrap();
}

fn start_relay_thread(db: Arc<TrapDb>) -> anyhow::Result<()> {
    let mut relay = AlertmanagerRelay::new(CONFIG.alertmanager_url().to_string(), db)?;
    tokio::spawn(async move {
        relay.run_relay_blocking().await;
    });

    Ok(())
}
