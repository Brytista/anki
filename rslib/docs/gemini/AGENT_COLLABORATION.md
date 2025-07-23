# Gemini Agent Collaboration Guide

> **Note:** This document is a collaborative workspace for Gemini agents and human developers. It serves as a high-level overview and progress tracker.
> For detailed, user-facing API documentation and usage examples, please see `DeveloperDocumentation.md`.

## Agent Workflow

This project utilizes two types of AI agents: a **Coding Agent** and a **Testing Agent**.

### Coding Agent Responsibilities

The primary goal for a Coding Agent is to add new features without conflicting with other agents working in parallel. This is achieved by isolating work to a new feature file and then safely updating a single, central file to register the new feature.

**The workflow is as follows:**

1.  **Create a new feature module:** Create a new file in `rslib/src/sync/http_server/rest_routes/` (e.g., `decks.rs`). This file will contain all logic for the new feature: handlers, request/response structs, and a local router.

2.  **Register the new feature atomically:** This is a critical step that must be performed carefully to avoid conflicts. The agent must modify `rslib/src/sync/http_server/rest_routes/mod.rs` in two specific places using the `replace` tool, which prevents overwriting other agents' changes.

    *   **A. Declare the new module:**
        *   Use `replace` to add the `mod` declaration after the last existing one.
        *   **`old_string`**: The last `mod` line (e.g., `mod cards;`)
        *   **`new_string`**: The last `mod` line + the new one (e.g., `mod cards;\nmod decks;`)

    *   **B. Merge the new router:**
        *   Use `replace` to add the `.merge()` call to the end of the router chain.
        *   **`old_string`**: The last `.merge()` line (e.g., `.merge(cards::routes())`)
        *   **`new_string`**: The last `.merge()` line + the new one (e.g., `.merge(cards::routes())\n        .merge(decks::routes())`)

    *If a `replace` call fails, it means another agent has updated the file. The agent must re-read the file and try again with the new content.*

3.  **Update the OpenAPI specification:** Modify `api.yaml` to reflect the new endpoints.

4.  **Add a testing checklist:** Add a new entry to the **API Endpoint Testing Checklist** section below for each new endpoint created.

**REMEMBER:** A strict constraint has been to reuse `rslib`'s core functions for all database interactions and business logic as much as possible, avoiding direct database manipulation in the API layer. If there is no existing `rslib`'s core function for database interaction and business logic, stop coding and inform user. 

### Testing Agent Responsibilities

1.  For each endpoint in the checklist, go through the testing steps one-by-one.
2.  For each step, document the exact command used for testing (e.g., `curl` command).
3.  Document the result of the command (e.g., HTTP status code and response body).
4.  Clearly identify whether the test case passed or failed based on the expected outcome. For failed tests, provide a brief note explaining the discrepancy.
5.  Mark the checkbox for the step once it has been successfully tested and documented.

    For example:

    - [x] **Happy Path:** A new card is created successfully with valid data.
        - **Status:** <font color='green'>Success</font>
        - **Command:**
        ```sh
        curl -X POST http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "deckName": "Default", "notetypeName": "Basic", "fields": { "Front": "Test Front", "Back": "Test Back" }, "tags": ["test-tag-1", "test-tag-2"] }'
        ```
        - **Result:**
        ```json
        {"card_ids":[1753268737657]}
        ```

    - [x] **Error Case (Invalid Deck):** The API handles requests for non-existent decks gracefully.
        - **Status:** <font color='red'>Fail</font>
        - **Command:**
        ```sh
        curl -X POST http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "deckName": "non-existent-deck", "notetypeName": "Basic", "fields": { "Front": "Test Front", "Back": "Test Back" }, "tags": ["test-tag-1", "test-tag-2"] }'
        ```
        - **Result:**
        ```json
        {"card_ids":[1753268804214]}
        ```
        - **Notes:** The API should have returned an error indicating the deck does not exist. Instead, it created a new deck and a new card.

## Project Overview

The goal of this project is to build a RESTful API on top of the `rslib` crate. This API will expose Anki's core functionality over HTTP, allowing for third-party integrations and new client applications. A strict constraint has been to reuse `rslib`'s core functions for all database interactions and business logic as much as possible, avoiding direct database manipulation in the API layer.

## Architecture

The API is built on a modular, feature-based routing architecture using Axum. This design is engineered to be scalable, maintainable, and to support parallel development by minimizing conflicts.

The architecture follows a clear chain of command:

1.  **The Main Server Switchboard (`rslib/src/sync/http_server/mod.rs`)**
    *   This is the highest-level router for the entire Anki server.
    *   It uses `.nest()` to delegate requests based on their URL prefix.
    *   Crucially, it directs all traffic starting with `/api/v1` to our REST API module, ensuring a clean separation from the legacy sync protocols (`/sync` and `/msync`).

2.  **The REST API Delegator (`rslib/src/sync/http_server/rest.rs`)**
    *   This file is the immutable entry point for all `/api/v1` routes.
    *   Its **only** job is to call the master routing function in the `rest_routes` module.
    *   **This file should never be modified.**

3.  **The Route Aggregator (`rslib/src/sync/http_server/rest_routes/mod.rs`)**
    *   This is the **single point of modification** for registering new features.
    *   It declares all feature modules (e.g., `mod cards;`, `mod decks;`).
    *   It builds and exposes a single master `routes()` function that merges the routers from all declared feature modules.

4.  **The Feature Modules (`rslib/src/sync/http_server/rest_routes/*.rs`)**
    *   Each `.rs` file in this directory (except `mod.rs`) is a self-contained feature module.
    *   It contains all the necessary logic: endpoint handlers, request/response structs, and a local Axum `Router` for its specific feature.

This layered design allows agents to work on different features in parallel by creating new files and then safely modifying the central `rest_routes/mod.rs` file using the atomic workflow described above.

## Progress Tracking

### `/cards` Endpoint: Flashcard Management

- [ ] **GET /api/v1/cards**: Search for cards using a flexible query language. This endpoint provides a powerful way to find specific cards based on a combination of criteria.
    - **Implementation:** This endpoint will accept a `q` query parameter containing a search string formatted according to the Anki search syntax. It will call `col.search_cards()` with the provided query and return a list of matching card IDs.
    - **Query Parameters:**
        - `q` (string, required): A URL-encoded string that follows the [Anki search syntax](https://docs.ankiweb.net/searching.html). Examples:
            - `deck:French tag:vocab is:due`
            - `"note:My Notetype" (front:hello OR back:world)`
            - `prop:reps>3`
- [x] **POST /api/v1/cards**: Allows adding new cards. This endpoint leverages `rslib` functions like `col.get_or_create_normal_deck()`, `col.get_notetype_by_name()`, `Note::new()`, `note.set_field()`, and `col.add_note()`.
- [x] **GET /api/v1/cards/{card_id}**: Allows retrieving detailed information about a specific card, including its rendered front and back HTML. This endpoint uses `rslib` functions such as `col.storage.get_card()` and `col.render_existing_card()`.
- [ ] **GET /api/v1/cards/{card_id}/stats**: Retrieve detailed statistics and history for a single card.
    - **Implementation:** This endpoint will call `col.card_stats()` with the specified card ID and return the resulting statistics object.
    - **Response Body:** The response should include details like the card's ease factor, number of reviews, lapses, and a history of past reviews.
- [x] **PUT /api/v1/cards/{card_id}**: Allows updating a card's content by modifying its associated note's fields and tags. This endpoint calls `col.update_note()`.
- [x] **DELETE /api/v1/cards**: Allows deleting one or more cards. This endpoint calls `col.remove_cards_and_orphaned_notes()`.
- [x] **PUT /api/v1/cards/{card_id}/schedule**: Allows manually adjusting a card's due date. This endpoint calls `col.set_due_date()` and correctly parses `+Nd` string formats.
- [ ] **POST /api/v1/cards/bury**: Temporarily hide cards from review.
    - **Implementation:** This endpoint should accept a list of card IDs and call `col.bury_or_suspend_cards()` with the `Bury` mode.
- [ ] **POST /api/v1/cards/suspend**: Suspend cards indefinitely.
    - **Implementation:** This endpoint should accept a list of card IDs and call `col.bury_or_suspend_cards()` with the `Suspend` mode.
- [ ] **POST /api/v1/cards/restore**: Restores one or more buried or suspended cards to their normal state, making them available for review again. This single endpoint handles both unburying and unsuspending cards.
    - **Implementation:** This endpoint should accept a list of card IDs and call `col.unbury_or_unsuspend_cards()`.
- [ ] **POST /api/v1/cards/forget**: Reset a card's learning progress.
    - **Implementation:** This endpoint should accept a list of card IDs and call `col.reschedule_cards_as_new()`.
- [ ] **PUT /api/v1/cards/deck**: Move cards to a different deck.
    - **Implementation:** This endpoint should accept a list of card IDs and a target deck ID, and call `col.set_deck()`.
- [ ] **PUT /api/v1/cards/flags**: Set the flag on a set of cards.
    - **Implementation:** This endpoint should accept a list of card IDs and a flag ID (0-7), and call `col.set_card_flag()`.
- [ ] **POST /api/v1/cards/{card_id}/answer**: Submit an answer for a card.
    - **Implementation:** This endpoint should accept a card ID and a rating (Again, Hard, Good, Easy) and call `col.answer_card()`.

### `/decks` Endpoint: Deck Management & Study

- [ ] **GET /api/v1/decks**: List all decks with their hierarchy and configuration.
    - **Implementation:** This endpoint should call `col.get_all_decks()` to retrieve a list of all decks in the collection.
- [ ] **POST /api/v1/decks**: Create a new deck.
    - **Implementation:** This endpoint should accept a deck name (including nested names like `"Parent::Child"`) and call `col.get_or_create_normal_deck()` to create it.
- [ ] **POST /api/v1/decks/import**: Import an Anki deck package (`.apkg` file).
    - **Implementation:** This endpoint will receive a multipart/form-data request containing the `.apkg` file. It will save the file to a temporary location and then call `col.import_apkg()` with the file path.
- [ ] **GET /api/v1/decks/{deck_id}**: Get detailed information and options for a specific deck.
    - **Implementation:** This endpoint should call `col.get_deck()` to retrieve the full configuration of a single deck.
- [ ] **GET /api/v1/decks/{deck_id}/export**: Export a specific deck to an Anki deck package (`.apkg`) file.
    - **Implementation:** This endpoint will call `col.export_apkg()` with a search query limited to the specified deck (e.g., `"deck:Deck Name"`). It will return the `.apkg` file as a binary download.
- [ ] **GET /api/v1/decks/{deck_id}/stats**: Get a statistical summary for a specific deck.
    - **Implementation:** This endpoint will gather statistics by making several calls to `col.search_cards()` with queries like `"deck:Deck Name" is:new`, `"deck:Deck Name" is:due`, etc., to get the counts for each card state. It will then assemble these counts into a summary object.
- [ ] **PUT /api/v1/decks/{deck_id}**: Update a deck's name and configuration options.
    - **Implementation:** This endpoint should accept a deck ID and a JSON body with the updated deck configuration, then call `col.update_deck()`.
- [ ] **DELETE /api/v1/decks/{deck_id}**: Delete a deck.
    - **Implementation:** This endpoint should call `col.remove_deck()` to delete the specified deck.
- [ ] **GET /api/v1/decks/{deck_id}/next-card**: Get the next card due for review in a deck.
    - **Implementation:** This endpoint should call `col.get_next_card()` for the specified deck.

### Cross-Cutting Concerns

- [ ] **Authentication**: Secure the API with a token-based authentication mechanism.

## API Endpoint Testing Checklist

This section is for manually tracking the testing of each API endpoint. For each endpoint, please verify the following:

### `POST /api/v1/cards`

- [x] **Happy Path:** A new card is created successfully with valid data.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X POST http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "deckName": "Default", "notetypeName": "Basic", "fields": { "Front": "Test Front", "Back": "Test Back" }, "tags": ["test-tag-1", "test-tag-2"] }'
    ```
    - **Result:**
    ```json
    {"card_ids":[1753272684209]}
    ```
- [x] **Error Case (Invalid Deck):** The API handles requests for non-existent decks gracefully.
    - **Status:** <font color='red'>Fail</font>
    - **Command:**
    ```sh
    curl -X POST http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "deckName": "non-existent-deck", "notetypeName": "Basic", "fields": { "Front": "Test Front", "Back": "Test Back" }, "tags": ["test-tag-1", "test-tag-2"] }'
    ```
    - **Result:**
    ```json
    {"card_ids":[1753272846121]}
    ```
    - **Notes:** The API should have returned an error indicating the deck does not exist. Instead, it created a new deck and a new card. This is because the underlying `get_or_create_normal_deck()` function creates a new deck if one with the given name does not exist.
- [x] **Error Case (Invalid Notetype):** The API handles requests for non-existent notetypes gracefully.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X POST http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "deckName": "Default", "notetypeName": "non-existent-notetype", "fields": { "Front": "Test Front", "Back": "Test Back" }, "tags": ["test-tag-1", "test-tag-2"] }'
    ```
    - **Result:**
    ```json
    {"error":{"code":400,"message":"Notetype not found: non-existent-notetype"}}
    ```

### `GET /api/v1/cards/{card_id}`

- [x] **Happy Path:** Card information is retrieved successfully for a valid card ID.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl http://localhost:8080/api/v1/cards/1753272684209
    ```
    - **Result:**
    ```json
    {"card_id":1753272684209,"deck_id":1,"due":8,"interval":0,"ease_factor":0.0,"rendered_front":"Test Front","rendered_back":"Test Front\n\n<hr id=answer>\n\nTest Back"}
    ```
- [x] **Error Case (Not Found):** The API returns a 404 error for a non-existent card ID.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl http://localhost:8080/api/v1/cards/1234567890
    ```
    - **Result:**
    ```json
    {"error":{"code":404,"message":"Your database appears to be in an inconsistent state. Please use the Check Database action. No such card: '1234567890'"}}
    ```

### `PUT /api/v1/cards/{card_id}`

- [x] **Happy Path:** A card's content is updated successfully with valid data.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X PUT http://localhost:8080/api/v1/cards/1753272684209 -H "Content-Type: application/json" -d '{ "fields": { "Front": "Updated Front", "Back": "Updated Back" }, "tags": ["updated-tag"] }'
    ```
    - **Result:**
    ```json
    {"success":true}
    ```
- [x] **Error Case (Not Found):** The API returns a 404 error for a non-existent card ID.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X PUT http://localhost:8080/api/v1/cards/1234567890 -H "Content-Type: application/json" -d '{ "fields": { "Front": "Updated Front", "Back": "Updated Back" }, "tags": ["updated-tag"] }'
    ```
    - **Result:**
    ```json
    {"error":{"code":404,"message":"Your database appears to be in an inconsistent state. Please use the Check Database action. No such card: '1234567890'"}}
    ```

### `PUT /api/v1/cards/{card_id}/schedule`

- [x] **Happy Path:** A card's schedule is updated successfully with a valid due date string.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X PUT http://localhost:8080/api/v1/cards/1753272684209/schedule -H "Content-Type: application/json" -d '{ "due": "+3d" }'
    ```
    - **Result:**
    ```json
    {"success":true}
    ```
- [x] **Error Case (Not Found):** The API returns a 404 error for a non-existent card ID.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X PUT http://localhost:8080/api/v1/cards/1234567890/schedule -H "Content-Type: application/json" -d '{ "due": "+3d" }'
    ```
    - **Result:**
    ```json
    {"error":{"code":404,"message":"Your database appears to be in an inconsistent state. Please use the Check Database action. No such card: '1234567890'"}}
    ```
- [x] **Error Case (Invalid Due Date):** The API handles invalid due date strings gracefully.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X PUT http://localhost:8080/api/v1/cards/1753274750333/schedule -H "Content-Type: application/json" -d '{ "due": "invalid-date" }'
    ```
    - **Result:**
    ```json
    {"error":{"code":400,"message":"invalid-date"}}
    ```

### `DELETE /api/v1/cards`

- [x] **Happy Path:** Cards are deleted successfully with a valid list of card IDs.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X DELETE http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "card_ids": [1753274750333] }'
    ```
    - **Result:**
    ```json
    {"success":true,"deleted_count":1}
    ```
- [x] **Error Case (Invalid ID):** The API handles requests with non-existent card IDs gracefully.
    - **Status:** <font color='green'>Success</font>
    - **Command:**
    ```sh
    curl -X DELETE http://localhost:8080/api/v1/cards -H "Content-Type: application/json" -d '{ "card_ids": [1234567890] }'
    ```
    - **Result:**
    ```json
    {"success":true,"deleted_count":0}
    ```

### `GET /api/v1/decks`

- [ ] **Happy Path:** A list of all decks is retrieved successfully.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```

### `POST /api/v1/decks`

- [ ] **Happy Path:** A new deck is created successfully.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```
- [ ] **Error Case (Conflict):** The API handles requests to create a deck that already exists.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```

### `GET /api/v1/decks/{deck_id}`

- [ ] **Happy Path:** Deck information is retrieved successfully for a valid deck ID.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```
- [ ] **Error Case (Not Found):** The API returns a 404 error for a non-existent deck ID.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```

### `PUT /api/v1/decks/{deck_id}`

- [ ] **Happy Path:** A deck is updated successfully with valid data.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```
- [ ] **Error Case (Not Found):** The API returns a 404 error for a non-existent deck ID.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```

### `DELETE /api/v1/decks/{deck_id}`

- [ ] **Happy Path:** A deck is deleted successfully.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```
- [ ] **Error Case (Not Found):** The API returns a 404 error for a non-existent deck ID.
    - **Command:**
    ```sh
    # (To be filled by Testing Agent)
    ```
    - **Result:**
    ```
    # (To be filled by Testing Agent)
    ```

## Common Issues and Pitfalls

This section documents common errors and their solutions to help agents avoid them in the future.

### Invalid Dynamic Route Syntax

*   **Symptom:** The server panics at startup with an error message similar to: `Path segments must not start with :. For capture groups, use {capture}`.
*   **Cause:** This occurs when defining a dynamic route using a colon prefix (e.g., `/:id`), which is a syntax used in other web frameworks or older versions of Axum.
*   **Solution:** This project's version of Axum requires brace-enclosed syntax for all dynamic path segments. Always use `{...}` for capture groups.
    *   **Incorrect:** `.route("/cards/:card_id", ...)`
    *   **Correct:** `.route("/cards/{card_id}", ...)`

## Links

*   [API Specification](./api.yaml)