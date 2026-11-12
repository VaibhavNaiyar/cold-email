use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use scout_db::Pool;
use serde_json::{Value, json};

use crate::draft::Writer;
use crate::refresh::JobRefresh;

#[derive(Clone)]
struct AppState {
    pool: Pool,
    writer: Arc<Writer>,
    refresh: Arc<JobRefresh>,
}

/// Read API for the web page. Bound to localhost: it has no sign-in yet.
pub async fn run(pool: Pool, writer: Writer, refresh: Arc<JobRefresh>, port: u16) -> Result<()> {
    let state = AppState { pool, writer: Arc::new(writer), refresh };
    let app = Router::new()
        .route("/api/companies", get(companies))
        .route("/api/companies/{id}/email", post(preview_email))
        .route("/api/jobs", get(jobs))
        .route("/api/jobs/refresh", get(refresh_status).post(start_refresh))
        .with_state(state);
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("API on http://{address}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn companies(State(state): State<AppState>) -> Response {
    json_or_failure(scout_db::board(&state.pool).await)
}

async fn jobs(State(state): State<AppState>) -> Response {
    json_or_failure(scout_db::job_listing(&state.pool).await)
}

async fn refresh_status(State(state): State<AppState>) -> Response {
    Json(state.refresh.status()).into_response()
}

/// Starts reading the careers boards again. 202 when started, 409 when a run is already going.
async fn start_refresh(State(state): State<AppState>) -> Response {
    let code = if state.refresh.start() { StatusCode::ACCEPTED } else { StatusCode::CONFLICT };
    (code, Json(state.refresh.status())).into_response()
}

fn json_or_failure(result: Result<Value>) -> Response {
    match result {
        Ok(rows) => Json(rows).into_response(),
        Err(error) => failure(&error),
    }
}

/// Writes the email this company would get, without saving it or touching Gmail.
async fn preview_email(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let company = match scout_db::draft_candidate(&state.pool, id).await {
        Ok(Some(company)) => company,
        Ok(None) => {
            let message = "No published address for this company yet, so there is no one to write to.";
            return (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response();
        }
        Err(error) => return failure(&error),
    };
    match state.writer.write(&company).await {
        Ok(Ok(email)) => Json(json!({
            "to": company.email,
            "subject": email.subject,
            "body": email.body,
        }))
        .into_response(),
        Ok(Err(problems)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "The email failed its checks twice.", "problems": problems })),
        )
            .into_response(),
        Err(error) => failure(&error),
    }
}

fn failure(error: &anyhow::Error) -> Response {
    eprintln!("API error: {error:#}");
    let body = Json(json!({ "error": format!("{error:#}") }));
    (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
}
