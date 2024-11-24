use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Mutex},
};

use speki_dto::{AttributeId, CardId};

use crate::{
    card::serializing::into_any, reviews::Reviews, AnyType, Attribute, Card, Provider, Recaller,
    TimeGetter,
};

#[derive(Clone)]
pub struct CardProvider {
    inner: Arc<Mutex<Inner>>,
    provider: Provider,
    time_provider: TimeGetter,
    recaller: Recaller,
}

impl Debug for CardProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardProvider")
            .field("inner", &":)")
            .finish()
    }
}

impl CardProvider {
    pub fn time_provider(&self) -> TimeGetter {
        self.time_provider.clone()
    }

    pub fn provider(&self) -> Provider {
        self.provider.clone()
    }

    pub fn recaller(&self) -> Recaller {
        self.recaller.clone()
    }

    pub fn new(provider: Provider, time_provider: TimeGetter, recaller: Recaller) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                cards: Default::default(),
            })),
            time_provider,
            provider,
            recaller,
        }
    }

    pub async fn load(&self, id: CardId) -> Option<Card<AnyType>> {
        let raw_card = self.provider.load_card(id).await?;
        let reviews = self.provider.load_reviews(id).await;
        let history = Reviews(reviews);
        let data = into_any(raw_card.data, self);

        let card = Card::<AnyType> {
            id,
            data,
            dependencies: raw_card.dependencies.into_iter().map(CardId).collect(),
            tags: raw_card.tags,
            history,
            suspended: crate::card::IsSuspended::from(raw_card.suspended),
            card_provider: self.clone(),
            recaller: self.recaller.clone(),
        };

        Some(card)
    }

    pub async fn load_attribute(&self, id: AttributeId) -> Option<Attribute> {
        self.provider
            .load_attribute(id)
            .await
            .map(|dto| Attribute::from_dto(dto, self.clone()))
    }
}

#[derive(Clone)]
struct Inner {
    cards: HashMap<CardId, Card<AnyType>>,
}
