use crate::alerts::Alert;
use crate::trap_db::TrapDb;
use actix_web::http::header;
use actix_web::web::{Data, Form, Html};
use actix_web::{HttpResponse, get, post};
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tera::{Context, Tera};

#[derive(Serialize)]
pub struct AlertView {
    pub hash: u64,
    pub severity: String,
    pub name: String,
    pub times: Vec<String>,
    pub labels: BTreeMap<String, String>,
    pub community: String,
}

impl From<&Alert> for AlertView {
    fn from(alert: &Alert) -> Self {
        let severity = alert.severity().to_string();
        let name = alert.pretty_name();
        let labels = alert.pretty_labels();
        let times = alert.times().iter().map(|t| t.to_string()).collect();

        AlertView {
            hash: alert.hash(),
            severity,
            name,
            times,
            labels,
            community: alert.community().to_string(),
        }
    }
}

#[get("/")]
async fn alerts_view(db: Data<TrapDb>, templates: Data<Tera>) -> Html {
    let mut alerts: Vec<AlertView> = db.cached_alerts().await.iter().map(Into::into).collect();
    alerts.sort_by(|a, b| a.times.cmp(&b.times));

    let mut ctx = Context::new();
    ctx.insert("alerts", &alerts);

    drop(alerts);

    let rendered = templates
        .render("alerts_view", &ctx)
        .expect("Builtin Template render failed");

    Html::new(rendered)
}

#[derive(Deserialize)]
struct AlertHash {
    hash: u64,
}

#[post("/api/clear")]
async fn clear_alert(db: Data<TrapDb>, Form(alert): Form<AlertHash>) -> HttpResponse {
    if let Err(e) = db.clear_alerts(alert.hash).await {
        error!("Failed to clear alerts: {}", e);
        return HttpResponse::InternalServerError()
            .insert_header((header::LOCATION, "/"))
            .finish();
    }

    HttpResponse::Ok()
        .insert_header((header::LOCATION, "/"))
        .finish()
}
