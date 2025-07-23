// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::Arc;

use axum::Router;

use super::rest_routes;
use crate::sync::http_server::SimpleServer;

/// The main router for the v1 REST API.
///
/// This function simply delegates to the master router in the `rest_routes` module.
/// This file should not be modified when adding new endpoints.
pub fn rest_router() -> Router<Arc<SimpleServer>> {
    rest_routes::routes()
}
