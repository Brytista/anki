// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::sync::LazyLock;

use rand::distr::Distribution;
use rand::distr::Uniform;
use regex::Regex;

use super::answering::CardAnswer;
use crate::card::Card;
use crate::card::CardId;
use crate::card::CardQueue;
use crate::card::CardType;
use crate::collection::Collection;
use crate::config::StringKey;
use crate::error::Result;
use crate::prelude::*;
use crate::scheduler::timing::is_unix_epoch_timestamp;

impl Card {
    /// Make card due in `days_from_today`.
    /// If card is not a review card, convert it into one.
    /// Review/relearning cards have their interval preserved unless
    /// `force_reset` is true.
    /// If the card has no ease factor (it's new), `ease_factor` is used.
    fn set_due_date(
        &mut self,
        today: u32,
        next_day_start: i64,
        days_from_today: u32,
        ease_factor: f32,
        force_reset: bool,
    ) {
        let new_due = (today + days_from_today) as i32;
        let fsrs_enabled = self.memory_state.is_some();
        let new_interval = if fsrs_enabled {
            if let Some(last_review_time) = self.last_review_time {
                let elapsed_days =
                    TimestampSecs(next_day_start).elapsed_days_since(last_review_time);
                elapsed_days as u32 + days_from_today
            } else {
                let due = self.original_or_current_due();
                let due_diff = if is_unix_epoch_timestamp(due) {
                    let offset = (due as i64 - next_day_start) / 86_400;
                    let due = (today as i64 + offset) as i32;
                    new_due - due
                } else {
                    new_due - due
                };
                self.interval.saturating_add_signed(due_diff)
            }
        } else if force_reset || !matches!(self.ctype, CardType::Review | CardType::Relearn) {
            days_from_today.max(1)
        } else {
            self.interval.max(1)
        };
        let ease_factor = (ease_factor * 1000.0).round() as u16;

        self.schedule_as_review(new_interval, new_due, ease_factor);
    }

    fn schedule_as_review(&mut self, interval: u32, due: i32, ease_factor: u16) {
        self.original_position = self.last_position();
        self.remove_from_filtered_deck_before_reschedule();
        self.interval = interval;
        self.due = due;
        self.ctype = CardType::Review;
        self.queue = CardQueue::Review;
        if self.ease_factor == 0 {
            // unlike the old Python code, we leave the ease factor alone
            // if it's already set
            self.ease_factor = ease_factor;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DueDateSpecifier {
    min: u32,
    max: u32,
    force_reset: bool,
}

pub fn parse_due_date_str(s: &str) -> Result<DueDateSpecifier> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)^
            # a number
            (?P<min>\d+)
            # an optional hyphen and another number
            (?:
                -
                (?P<max>\d+)
            )?
            # optional exclamation mark
            (?P<bang>!)?
            $
        ",
        )
        .unwrap()
    });
    let caps = RE.captures(s).or_invalid(s)?;
    let min: u32 = caps.name("min").unwrap().as_str().parse()?;
    let max = if let Some(max) = caps.name("max") {
        max.as_str().parse()?
    } else {
        min
    };
    let force_reset = caps.name("bang").is_some();
    Ok(DueDateSpecifier {
        min: min.min(max),
        max: max.max(min),
        force_reset,
    })
}

impl Collection {
    /// `days` should be in a format parseable by `parse_due_date_str`.
    /// If `context` is provided, provided key will be updated with the new
    /// value of `days`.
    pub fn set_due_date(
        &mut self,
        cids: &[CardId],
        days: &str,
        context: Option<StringKey>,
    ) -> Result<OpOutput<()>> {
        let spec = parse_due_date_str(days)?;
        if cids.is_empty() {
            return Ok(OpOutput {
                output: (),
                changes: Default::default(),
            });
        }
        let usn = self.usn()?;
        let today = self.timing_today()?.days_elapsed;
        let next_day_start = self.timing_today()?.next_day_at.0;
        let mut rng = rand::rng();
        let distribution = Uniform::new_inclusive(spec.min, spec.max).unwrap();
        let mut decks_initial_ease: HashMap<DeckId, f32> = HashMap::new();
        self.transact(Op::SetDueDate, |col| {
            let cards = col.all_cards_for_ids(cids, false)?;
            if cards.len() != cids.len() {
                let found_cids: std::collections::HashSet<CardId> =
                    cards.iter().map(|c| c.id).collect();
                let missing_cid = cids.iter().find(|cid| !found_cids.contains(cid)).unwrap();

                return Err(AnkiError::NotFound {
                    source: crate::error::NotFoundError {
                        type_name: "card".to_string(),
                        identifier: missing_cid.to_string(),
                        backtrace: None,
                    },
                });
            }
            for mut card in cards {
                let deck_id = card.original_deck_id.or(card.deck_id);
                let ease_factor = match decks_initial_ease.get(&deck_id) {
                    Some(ease) => *ease,
                    None => {
                        let deck = col.get_deck(deck_id)?.or_not_found(deck_id)?;
                        let config_id = deck.config_id().or_invalid("home deck is filtered")?;
                        let ease = col
                            .get_deck_config(config_id, true)?
                            // just for compiler; get_deck_config() is guaranteed to return a value
                            .unwrap_or_default()
                            .inner
                            .initial_ease;
                        decks_initial_ease.insert(deck_id, ease);
                        ease
                    }
                };
                let original = card.clone();
                let days_from_today = distribution.sample(&mut rng);
                card.set_due_date(
                    today,
                    next_day_start,
                    days_from_today,
                    ease_factor,
                    spec.force_reset,
                );
                col.log_manually_scheduled_review(&card, original.interval, usn)?;
                col.update_card_inner(&mut card, original, usn)?;
            }
            if let Some(key) = context {
                col.set_config_string_inner(key, days)?;
            }
            Ok(())
        })
    }

    pub fn grade_now(&mut self, cids: &[CardId], rating: i32) -> Result<OpOutput<()>> {
        self.transact(Op::GradeNow, |col| {
            for &card_id in cids {
                let states = col.get_scheduling_states(card_id)?;
                let new_state = match rating {
                    0 => states.again,
                    1 => states.hard,
                    2 => states.good,
                    3 => states.easy,
                    _ => invalid_input!("invalid rating"),
                };
                let mut answer: CardAnswer = anki_proto::scheduler::CardAnswer {
                    card_id: card_id.into(),
                    current_state: Some(states.current.into()),
                    new_state: Some(new_state.into()),
                    rating,
                    milliseconds_taken: 0,
                    answered_at_millis: TimestampMillis::now().into(),
                }
                .into();
                // Process the card without updating queues yet
                answer.from_queue = false;
                col.answer_card_inner(&mut answer)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn parse() -> Result<()> {
        type S = DueDateSpecifier;
        assert!(parse_due_date_str("").is_err());
        assert!(parse_due_date_str("x").is_err());
        assert!(parse_due_date_str("-5").is_err());
        assert_eq!(
            parse_due_date_str("5")?,
            S {
                min: 5,
                max: 5,
                force_reset: false
            }
        );
        assert_eq!(
            parse_due_date_str("5!")?,
            S {
                min: 5,
                max: 5,
                force_reset: true
            }
        );
        assert_eq!(
            parse_due_date_str("50-70")?,
            S {
                min: 50,
                max: 70,
                force_reset: false
            }
        );
        assert_eq!(
            parse_due_date_str("70-50!")?,
            S {
                min: 50,
                max: 70,
                force_reset: true
            }
        );
        Ok(())
    }

    #[test]
    fn due_date() {
        let mut c = Card::new(NoteId(0), 0, DeckId(0), 0);

        // setting the due date of a new card will convert it
        c.set_due_date(5, 0, 2, 1.8, false);
        assert_eq!(c.ctype, CardType::Review);
        assert_eq!(c.due, 7);
        assert_eq!(c.interval, 2);
        assert_eq!(c.ease_factor, 1800);

        // reschedule it again the next day, shifting it from day 7 to day 9
        c.set_due_date(6, 0, 3, 2.5, false);
        assert_eq!(c.due, 9);
        assert_eq!(c.interval, 2);
        assert_eq!(c.ease_factor, 1800); // interval doesn't change

        // we can bring cards forward too - return it to its original due date
        c.set_due_date(6, 0, 1, 2.4, false);
        assert_eq!(c.due, 7);
        assert_eq!(c.interval, 2);
        assert_eq!(c.ease_factor, 1800); // interval doesn't change

        // we can force the interval to be reset instead of shifted
        c.set_due_date(6, 0, 3, 2.3, true);
        assert_eq!(c.due, 9);
        assert_eq!(c.interval, 3);
        assert_eq!(c.ease_factor, 1800); // interval doesn't change

        // should work in a filtered deck
        c.interval = 2;
        c.ease_factor = 0;
        c.original_due = 7;
        c.original_deck_id = DeckId(1);
        c.due = -10000;
        c.queue = CardQueue::New;
        c.set_due_date(6, 0, 1, 2.2, false);
        assert_eq!(c.due, 7);
        assert_eq!(c.interval, 2);
        assert_eq!(c.ease_factor, 2200);
        assert_eq!(c.queue, CardQueue::Review);
        assert_eq!(c.original_due, 0);
        assert_eq!(c.original_deck_id, DeckId(0));

        // relearning treated like review
        c.ctype = CardType::Relearn;
        c.original_due = c.due;
        c.due = 12345678;
        c.set_due_date(6, 0, 10, 2.1, false);
        assert_eq!(c.due, 16);
        assert_eq!(c.interval, 2);
        assert_eq!(c.ease_factor, 2200); // interval doesn't change
    }
}
