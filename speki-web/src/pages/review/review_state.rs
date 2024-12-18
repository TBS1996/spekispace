use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use speki_core::{AnyType, Card};
use speki_dto::{CardId, Recall};
use tracing::info;

use crate::{components::GraphRep, App, DEFAULT_FILTER};

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
    pub filter: Signal<String>,
    pub graph: GraphRep,
}

impl ReviewState {
    pub fn new(app: App, graph: GraphRep) -> Self {
        Self {
            app,
            card: Default::default(),
            queue: Default::default(),
            tot_len: Default::default(),
            pos: Default::default(),
            front: Default::default(),
            back: Default::default(),
            show_backside: Default::default(),
            filter: Signal::new(DEFAULT_FILTER.to_string()),
            graph,
        }
    }

    pub async fn refresh(&mut self) {
        info!("refreshing..");
        let filter = self.filter.cloned();
        let cards = self.app.0.load_non_pending(Some(filter)).await;
        info!("review cards loaded");
        self.tot_len.clone().set(cards.len());
        {
            info!("setting queue");
            let mut lock = self.queue.lock().unwrap();
            *lock = cards;
            info!("queue was set");
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

        if let Some(card) = card.as_ref() {
            let card = Arc::new(card.clone());
            self.graph.new_set_card(card);
        }

        info!("card set: {:?}", card);
        self.card.clone().set(card);
        self.pos.clone().set(self.current_pos());
        self.show_backside.set(false);
    }
}
