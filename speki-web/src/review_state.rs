use crate::{App, REPO_PATH};
use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::{CardId, Review as ReviewDTO, SpekiProvider};
use std::sync::{Arc, Mutex};
use tracing::instrument;

use speki_idb::IndexBaseProvider;

#[derive(Default, Clone, Debug)]
pub struct ReviewState {
    pub card: Signal<Option<Card<AnyType>>>,
    pub queue: Arc<Mutex<Vec<CardId>>>,
    pub tot_len: Signal<usize>,
    pub pos: Signal<usize>,
    pub front: Signal<String>,
    pub back: Signal<String>,
    pub show_backside: Signal<bool>,
}

impl ReviewState {
    fn id(&self) -> Option<CardId> {
        Some(self.card.as_ref()?.id())
    }

    #[instrument]
    pub async fn refresh(&mut self, filter: String) {
        let app = use_context::<App>();
        let cards = app.0.load_non_pending(Some(filter)).await;
        self.tot_len.clone().set(cards.len());
        {
            let mut lock = self.queue.lock().unwrap();
            *lock = cards;
        }
        self.next_card(REPO_PATH).await;
    }

    async fn make_review(&self, review: ReviewDTO, repo: &str) {
        if let Some(id) = self.id() {
            IndexBaseProvider::new(repo).add_review(id, review).await;
        }
    }

    fn current_pos(&self) -> usize {
        self.tot_len - self.queue.lock().unwrap().len()
    }

    pub async fn do_review(&mut self, review: ReviewDTO) {
        let repo = REPO_PATH;
        self.make_review(review, repo).await;
        self.next_card(repo).await;
    }

    async fn next_card(&mut self, repo: &str) {
        let app = use_context::<App>();
        let card = self.queue.lock().unwrap().pop();

        let card = match card {
            Some(id) => {
                let card = Card::from_raw(
                    IndexBaseProvider::new(repo).load_card(id).await.unwrap(),
                    app.0.card_provider.clone(),
                    app.0.recaller.clone(),
                )
                .await;
                let front = card.print().await;
                let back = card
                    .display_backside()
                    .await
                    .unwrap_or_else(|| "___".to_string());

                self.front.clone().set(front);
                self.back.clone().set(back);
                Some(card)
            }
            None => None,
        };

        self.card.clone().set(card);
        self.pos.clone().set(self.current_pos());
        self.show_backside.set(false);
    }
}
