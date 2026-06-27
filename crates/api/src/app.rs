use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    middleware::auth::auth_layer,
    routes::{
        api_keys::{create_api_key, list_api_keys},
        chat::chat_completions,
        connections::{calibrate_connection, create_connection, delete_connection, list_connections, list_profiles},
        health::health,
        models::list_models,
        orchestrate::orchestrate,
        runs::{get_run, get_run_plan, get_run_trace, list_runs},
        usage::{usage_recent, usage_stats},
    },
    state::AppState,
};

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // OpenAI-compatible routes (require API key auth when enabled)
    let oai_routes = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/orchestrate", post(orchestrate))
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer));

    // Admin routes (dashboard / management)
    let admin_routes = Router::new()
        .route("/admin/api-keys", post(create_api_key))
        .route("/admin/api-keys", get(list_api_keys))
        .route("/admin/connections", post(create_connection))
        .route("/admin/connections", get(list_connections))
        .route("/admin/connections/:id", delete(delete_connection))
        .route("/admin/connections/:id/calibrate", post(calibrate_connection))
        .route("/admin/connections/:id/profiles", get(list_profiles))
        .route("/admin/usage/stats", get(usage_stats))
        .route("/admin/usage/recent", get(usage_recent))
        .route("/admin/runs", get(list_runs))
        .route("/admin/runs/:id", get(get_run))
        .route("/admin/runs/:id/trace", get(get_run_trace))
        .route("/admin/runs/:id/plan", get(get_run_plan));

    Router::new()
        .route("/health", get(health))
        .merge(oai_routes)
        .merge(admin_routes)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
