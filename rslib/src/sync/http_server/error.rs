
// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{
    error::AnkiError,
    prelude::I18n,
    sync::error::HttpError,
};

// Error handling
pub enum ApiError {
    Anki(AnkiError),
    Json(JsonRejection),
    Http(HttpError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::Anki(err) => {
                let status = match &err {
                    AnkiError::NotFound { .. } => StatusCode::NOT_FOUND,
                    AnkiError::InvalidInput { .. } => StatusCode::BAD_REQUEST,
                    AnkiError::Existing { .. } => StatusCode::CONFLICT,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, status.as_u16(), err.message(&I18n::template_only()))
            }
            ApiError::Json(err) => (
                StatusCode::BAD_REQUEST,
                StatusCode::BAD_REQUEST.as_u16(),
                err.body_text(),
            ),
            ApiError::Http(err) => (err.code, err.code.as_u16(), err.context),
        };
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}

impl From<AnkiError> for ApiError {
    fn from(err: AnkiError) -> Self {
        ApiError::Anki(err)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(err: JsonRejection) -> Self {
        ApiError::Json(err)
    }
}

impl From<HttpError> for ApiError {
    fn from(err: HttpError) -> Self {
        ApiError::Http(err)
    }
}
