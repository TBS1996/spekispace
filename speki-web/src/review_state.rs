use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::{CardId, Recall};
use tracing::{info, instrument};

use crate::App;

#[derive(Clone, Debug)]
pub struct ReviewState {
    pub app: App,
    pub card: Signal<Option<Card<AnyType>>>,
    pub queue: Arc<Mutex<Vec<CardId>>>,
    pub tot_len: Signal<usize>,
    pub pos: Signal<usize>,
    pub front: Signal<String>,
    pub back: Signal<String>,
    pub show_backside: Signal<bool>,
}

impl ReviewState {
    pub fn new(app: App) -> Self {
        Self {
            app,
            card: Default::default(),
            queue: Default::default(),
            tot_len: Default::default(),
            pos: Default::default(),
            front: Default::default(),
            back: Default::default(),
            show_backside: Default::default(),
        }
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
        self.next_card().await;
    }

    async fn make_review(&self, recall: Recall) {
        info!("make review");
        self.card.cloned().unwrap().add_review(recall).await;
    }

    fn current_pos(&self) -> usize {
        self.tot_len - self.queue.lock().unwrap().len()
    }

    pub async fn do_review(&mut self, review: Recall) {
        info!("do review");
        self.make_review(review).await;
        self.next_card().await;
    }

    async fn next_card(&mut self) {
        let card = self.queue.lock().unwrap().pop();
        let card = match card {
            Some(id) => {
                let card = self.app.0.load_card(id).await.unwrap();
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
