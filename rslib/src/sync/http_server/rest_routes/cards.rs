
// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{rejection::JsonRejection, Path, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    card::CardId,
    collection::Collection,
    error::{AnkiError, InvalidInputError},
    notes::Note,
    prelude::*,
    sync::http_server::{ApiResult, SimpleServer},
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

#[derive(Deserialize)]
pub struct UpdateCardContentRequest {
    fields: HashMap<String, String>,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct UpdateScheduleRequest {
    due: String,
}

#[derive(Deserialize)]
pub struct DeleteCardsRequest {
    card_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    success: bool,
}

#[derive(Serialize)]
pub struct DeleteCardsResponse {
    success: bool,
    deleted_count: usize,
}

// Router definition
pub fn routes() -> Router<Arc<SimpleServer>> {
    Router::new()
        .route("/cards", post(add_card).delete(delete_cards))
        .route("/cards/{card_id}", get(get_card).put(update_card_content))
        .route("/cards/{card_id}/schedule", put(update_schedule))
}

fn with_col<F, T>(server: &SimpleServer, op: F) -> ApiResult<T>
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
    payload: Result<Json<AddCardRequest>, JsonRejection>,
) -> ApiResult<Json<AddCardResponse>> {
    let payload = payload?;
    with_col(&server, |col| {
        let deck_id = col.get_or_create_normal_deck(&payload.deck_name)?.id;
        let notetype = col
            .get_notetype_by_name(&payload.notetype_name)?
            .ok_or_else(|| AnkiError::InvalidInput {
                source: InvalidInputError {
                    message: format!("Notetype not found: {}", payload.notetype_name),
                    source: None,
                    backtrace: None,
                },
            })?;

        let mut note = Note::new(&notetype);
        note.tags = payload.tags.clone();

        for (name, value) in &payload.fields {
            if let Some(idx) = notetype.get_field_ord(name) {
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
) -> ApiResult<Json<CardInfoResponse>> {
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

// Handler for updating a card's content
async fn update_card_content(
    State(server): State<Arc<SimpleServer>>,
    Path(card_id): Path<i64>,
    payload: Result<Json<UpdateCardContentRequest>, JsonRejection>,
) -> ApiResult<Json<SuccessResponse>> {
    let payload = payload?;
    with_col(&server, |col| {
        let cid = CardId(card_id);
        let card = col.storage.get_card(cid)?.ok_or(AnkiError::NotFound {
            source: crate::error::NotFoundError {
                type_name: "card".to_string(),
                identifier: cid.to_string(),
                backtrace: None,
            },
        })?;
        let mut note = col
            .storage
            .get_note(card.note_id)?
            .ok_or(AnkiError::NotFound {
                source: crate::error::NotFoundError {
                    type_name: "note".to_string(),
                    identifier: card.note_id.to_string(),
                    backtrace: None,
                },
            })?;
        let notetype = col.get_notetype(note.notetype_id)?.unwrap();

        for (name, value) in &payload.fields {
            if let Some(idx) = notetype.get_field_ord(name) {
                note.set_field(idx, value)?;
            }
        }

        if let Some(tags) = &payload.tags {
            note.tags = tags.clone();
        }

        col.update_note(&mut note)?;

        Ok(Json(SuccessResponse { success: true }))
    })
}

// Handler for updating a card's schedule
async fn update_schedule(
    State(server): State<Arc<SimpleServer>>,
    Path(card_id): Path<i64>,
    payload: Result<Json<UpdateScheduleRequest>, JsonRejection>,
) -> ApiResult<Json<SuccessResponse>> {
    let payload = payload?;
    with_col(&server, |col| {
        let cid = CardId(card_id);
        let due_str = if let Some(days) = payload
            .due
            .strip_prefix('+')
            .and_then(|s| s.strip_suffix('d'))
        {
            days.to_string()
        } else {
            payload.due.clone()
        };
        col.set_due_date(&[cid], &due_str, None)?;
        Ok(Json(SuccessResponse { success: true }))
    })
}

// Handler for deleting cards
async fn delete_cards(
    State(server): State<Arc<SimpleServer>>,
    payload: Result<Json<DeleteCardsRequest>, JsonRejection>,
) -> ApiResult<Json<DeleteCardsResponse>> {
    let payload = payload?;
    with_col(&server, |col| {
        let cids: Vec<CardId> = payload.card_ids.clone().into_iter().map(CardId).collect();
        let count = col.remove_cards_and_orphaned_notes(&cids)?;
        Ok(Json(DeleteCardsResponse {
            success: true,
            deleted_count: count,
        }))
    })
}
