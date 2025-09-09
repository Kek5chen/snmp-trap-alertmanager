mod alertmanager;
pub mod alerts;
pub mod config;
pub mod sanitize;
pub mod trap_db;
pub mod web;

use crate::alertmanager::AlertmanagerRelay;
use crate::config::CONFIG;
use crate::trap_db::TrapDb;
use crate::web::{alerts_view, clear_alert};
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use std::sync::Arc;
use tera::Tera;

#[tokio::main]
async fn main() {
    _ = dotenvy::dotenv();
    env_logger::init();

    let db = TrapDb::new(CONFIG.db_url()).unwrap();

    let mut tera = Tera::default();
    tera.add_raw_template("alerts_view", include_str!("../templates/alerts.html"))
        .expect("Failed to add built-in alert template");

    let shared_db = Arc::new(db);
    let shared_tera = Arc::new(tera);

    start_relay_thread(shared_db.clone());
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

fn start_relay_thread(db: Arc<TrapDb>) {
    tokio::spawn(async move {
        AlertmanagerRelay::new(CONFIG.alertmanager_url().to_string(), db)
            .run_relay_blocking()
            .await;
    });
}
