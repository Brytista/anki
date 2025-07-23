// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::Arc;

use axum::Router;

use crate::sync::http_server::SimpleServer;

// Declare feature modules
mod cards;

/// The master router for all REST API endpoints.
pub fn routes() -> Router<Arc<SimpleServer>> {
    Router::new().merge(cards::routes())
}
