// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    card::CardId,
    collection::Collection,
    decks::DeckId,
    error::{AnkiError, OrInvalid},
    i18n::I18n,
    notes::Note,
    prelude::*,
    sync::error::HttpError,
    sync::http_server::{user::User, SimpleServer},
};

// Payloads for the API
#[derive(Deserialize)]
pub struct AddCardRequest {
    #[serde(rename = "deckName")]
    deck_name: String,
    #[serde(rename = "notetypeName")]
    notetype_name: String,
    fields: HashMap<String, String>,
    tags: Vec<String>,
}

#[derive(Serialize)]
pub struct AddCardResponse {
    card_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct CardInfoResponse {
    card_id: i64,
    deck_id: i64,
    due: i32,
    interval: u32,
    ease_factor: f32,
    rendered_front: String,
    rendered_back: String,
}

// Error handling
pub struct RestError(AnkiError);

impl IntoResponse for RestError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self.0 {
            AnkiError::NotFound { .. } => (StatusCode::NOT_FOUND, self.0.message(&I18n::template_only())),
            AnkiError::InvalidInput { .. } => (StatusCode::BAD_REQUEST, self.0.message(&I18n::template_only())),
            AnkiError::Existing => (StatusCode::CONFLICT, self.0.message(&I18n::template_only())),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()),
        };
        (status, Json(json!({ "error": error_message }))).into_response()
    }
}

impl<E> From<E> for RestError
where
    E: Into<AnkiError>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl From<HttpError> for RestError {
    fn from(err: HttpError) -> Self {
        Self(AnkiError::invalid_input(err.to_string()))
    }
}

// Router definition
pub fn rest_router() -> Router<Arc<SimpleServer>> {
    Router::new()
        .route("/cards", post(add_card))
        .route("/cards/:card_id", get(get_card))
}

fn with_col<F, T>(server: &SimpleServer, op: F) -> Result<T, RestError>
where
    F: FnOnce(&mut Collection) -> Result<T, AnkiError>,
{
    let mut state = server.state.lock().unwrap();
    // For now, we'll just grab the first user.
    let user = state.users.values_mut().next().unwrap();
    user.ensure_col_open()?;
    let col = user.col.as_mut().unwrap();
    op(col).map_err(Into::into)
}

// Handler for adding a card
async fn add_card(
    State(server): State<Arc<SimpleServer>>,
    Json(payload): Json<AddCardRequest>,
) -> Result<Json<AddCardResponse>, RestError> {
    with_col(&server, |col| {
        let deck_id = col.get_or_create_normal_deck(&payload.deck_name)?.id;
        let notetype = col
            .get_notetype_by_name(&payload.notetype_name)?
            .ok_or_else(|| AnkiError::invalid_input(format!("Notetype not found: {}", payload.notetype_name)))?;

        let mut note = Note::new(&notetype);
        note.tags = payload.tags;

        for (name, value) in payload.fields {
            if let Some(idx) = notetype.get_field_ord(&name) {
                note.set_field(idx, value)?;
            }
        }

        col.add_note(&mut note, deck_id)?;

        let card_ids = col.storage.card_ids_of_notes(&[note.id])?;

        Ok(Json(AddCardResponse {
            card_ids: card_ids.into_iter().map(|id| id.0).collect(),
        }))
    })
}

// Handler for getting a card
async fn get_card(
    State(server): State<Arc<SimpleServer>>,
    Path(card_id): Path<i64>,
) -> Result<Json<CardInfoResponse>, RestError> {
    with_col(&server, |col| {
        let cid = CardId(card_id);
        let card = col.storage.get_card(cid)?.ok_or(AnkiError::NotFound {
            source: crate::error::NotFoundError {
                type_name: "card".to_string(),
                identifier: cid.to_string(),
                backtrace: None,
            },
        })?;
        let rendered = col.render_existing_card(cid, false, false)?;

        Ok(Json(CardInfoResponse {
            card_id: card.id.0,
            deck_id: card.deck_id.0,
            due: card.due,
            interval: card.interval,
            ease_factor: card.ease_factor(),
            rendered_front: rendered.question().to_string(),
            rendered_back: rendered.answer().to_string(),
        }))
    })
}
