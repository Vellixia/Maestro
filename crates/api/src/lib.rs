#![allow(clippy::result_large_err)]

pub mod app;
pub mod background;
pub mod error;
pub mod middleware;
pub mod routes;
pub mod state;

pub use app::build_router;
pub use state::AppState;
