use core::f32;
use std::{
    cmp::{Ord, Ordering, PartialEq},
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Debug,
    ops::Deref,
    sync::Arc,
    time::Duration,
};

pub mod basecard;
pub use basecard::*;

use either::Either;
use ledgerstore::{EventError, RefGetter, TheCacheGetter, TheLedgerAction, TimeProvider};
use nonempty::NonEmpty;
use serde::Deserializer;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    audio::{Audio, AudioId},
    card_provider::CardProvider,
    ledger::{CardAction, CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, Recall, Review, ReviewAction, ReviewEvent, SimpleRecall},
    CardRefType, FsTime,
};

pub type RecallRate = f32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvalText {
    cmps: Vec<Either<String, (String, CardId)>>,
    eval: String,
}

impl EvalText {
    pub fn components(&self) -> &Vec<Either<String, (String, CardId)>> {
        &self.cmps
    }

    pub fn just_some_ref(id: CardId, provider: &CardProvider) -> Self {
        let mut txt = TextData::default();
        txt.push_link(id, None);

        Self::from_textdata(txt, provider)
    }

    pub fn just_some_string(s: String, provider: &CardProvider) -> Self {
        Self::from_textdata(TextData::from_raw(&s), provider)
    }

    pub fn from_backside(b: &BackSide, provider: &CardProvider, hint: bool) -> Self {
        match b {
            BackSide::Text(txt) => Self::from_textdata(txt.clone(), provider),
            BackSide::Card(id) => {
                let eval = provider.load(*id).unwrap().frontside.to_string();
                Self {
                    cmps: if hint {
                        vec![
                            Either::Left("🔗".to_string()),
                            Either::Right((eval.clone(), *id)),
                        ]
                    } else {
                        vec![Either::Right((eval.clone(), *id))]
                    },
                    eval,
                }
            }
            BackSide::List(ids) => {
                let mut txt = TextData::default();

                for id in ids {
                    txt.inner_mut().push(Either::Right(TextLink {
                        id: *id,
                        alias: None,
                    }));
                    txt.inner_mut().push(Either::Left(", ".to_string()));
                }

                txt.inner_mut().pop();

                Self::from_textdata(txt, provider)
            }
            BackSide::Time(ts) => {
                if hint {
                    Self::just_some_string(
                        format!("{} {}", ts.clock_emoji(), ts.to_string()),
                        provider,
                    )
                } else {
                    Self::just_some_string(format!("{}", ts.to_string()), provider)
                }
            }
            BackSide::Trivial => Self::just_some_string("<trivial>".to_string(), provider),
            BackSide::Invalid => Self::just_some_string("<invalid>".to_string(), provider),
            BackSide::Bool(b) => Self::just_some_string(
                if hint {
                    format!(
                        "🔘 {}",
                        if *b {
                            "yes".to_string()
                        } else {
                            "no".to_string()
                        }
                    )
                } else {
                    format!(
                        "{}",
                        if *b {
                            "yes".to_string()
                        } else {
                            "no".to_string()
                        }
                    )
                },
                provider,
            ),
        }
    }

    pub fn from_textdata(txt: TextData, provider: &CardProvider) -> Self {
        let mut cmps = vec![];

        let eval = txt.evaluate(provider);

        for cmp in txt.inner() {
            match cmp {
                Either::Left(s) => cmps.push(Either::Left(s.to_string())),
                Either::Right(TextLink { id, alias }) => match alias {
                    Some(alias) => {
                        cmps.push(Either::Right((alias.to_string(), *id)));
                    }
                    None => {
                        match provider.load(*id) {
                            Some(card) => {
                                let name = card.name.to_string();
                                cmps.push(Either::Right((name, *id)));
                            }
                            None => panic!(),
                        };
                    }
                },
            }
        }

        Self { cmps, eval }
    }
}

impl Deref for EvalText {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.eval
    }
}

/*

hmm maybe we can go back to having generics here?
like a marker generic only. doesn't actually do anything.

we can have these empty unit structs, like Attribute, Class, Normal

and one called Any

and then we can simply set Any to be the default generic so it won't actually affect any existing code.

*/

#[derive(Clone)]
pub struct Card {
    id: CardId,
    namespace: Option<CardId>,
    front_audio: Option<Audio>,
    back_audio: Option<Audio>,
    name: EvalText,
    frontside: EvalText,
    backside: EvalText,
    base: RawCard,
    metadata: Metadata,
    history: History,
    card_provider: CardProvider,
    recaller: SimpleRecall,
}

impl PartialEq for Card {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Card {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for Card {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Eq for Card {}

impl Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        s.push_str(&format!("{:?}\n", self.id));
        s.push_str(&format!("{:?}\n", self.base.data.type_name()));
        s.push_str(&format!("{:?}\n", "omg"));

        write!(f, "{s}")
    }
}

impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.print())
    }
}

impl Card {
    pub fn trivial(&self) -> bool {
        if let Some(flag) = self.metadata.trivial {
            flag
        } else {
            self.base.trivial
        }
    }

    pub fn params_on_class(&self) -> Vec<Attrv2> {
        if let CardType::Class { params, .. } = &self.base.data {
            params.values().cloned().collect()
        } else {
            Default::default()
        }
    }

    pub fn param_answers(&self) -> BTreeMap<AttributeId, ParamAnswer> {
        if let CardType::Instance {
            answered_params, ..
        } = &self.base.data
        {
            answered_params.clone()
        } else {
            Default::default()
        }
    }

    pub fn reviewable(&self) -> bool {
        self.is_finished() && !self.trivial() && self.back_side().is_some()
    }

    pub fn clone_base(&self) -> RawCard {
        self.base.clone()
    }

    pub fn attributes_on_class(&self) -> Option<Vec<Attrv2>> {
        if let CardType::Class { attrs, .. } = self.base.clone().data {
            return Some(attrs.into_iter().collect());
        } else {
            None
        }
    }

    pub fn attributes(&self) -> Option<Vec<Attrv2>> {
        if !self.is_instance() && !self.is_class() {
            return None;
        };

        let mut output = vec![];

        for class in self.parent_classes() {
            let card = self.card_provider.providers.cards.load(class).unwrap();
            if let CardType::Class { attrs, .. } = card.data {
                output.extend(attrs);
            }
        }

        Some(output)
    }

    pub fn class(&self) -> Option<CardId> {
        match &self.base.data {
            CardType::Instance { class, .. } => Some(*class),
            CardType::Class { parent_class, .. } => *parent_class,
            _ => None,
        }
    }

    pub fn parent_classes(&self) -> HashSet<CardId> {
        let key = match self.base.data {
            CardType::Instance { class, .. } => class,
            CardType::Class { .. } => self.id,
            _ => panic!(),
        };

        let getter = TheCacheGetter::ItemRef(RefGetter {
            reversed: false,
            ty: Some(CardRefType::ParentClass),
            key,
            recursive: true,
        });

        let mut classes = self.card_provider.providers.cards.load_getter(getter);
        classes.insert(key);
        classes
    }

    pub fn get_attr(&self, id: AttributeId) -> Option<Attrv2> {
        self.attributes()?.into_iter().find(|attr| attr.id == id)
    }

    pub fn attr_id(&self) -> Option<AttributeId> {
        if let CardType::Attribute { attribute, .. } = &self.base.data {
            Some(*attribute)
        } else {
            None
        }
    }

    /// gets the instance that this attribute card is based on.
    pub fn attribute_instance(&self) -> CardId {
        if let CardType::Attribute { instance, .. } = self.base.data {
            instance
        } else {
            dbg!(self);
            panic!("card must be of type attribute");
        }
    }

    pub fn uses_attr_id(&self, id: AttributeId) -> bool {
        self.attr_id().is_some_and(|attr_id| attr_id == id)
    }

    pub fn front_audio(&self) -> Option<&Audio> {
        self.front_audio.as_ref()
    }

    pub fn back_audio(&self) -> Option<&Audio> {
        self.back_audio.as_ref()
    }

    pub fn front_audio_id(&self) -> Option<AudioId> {
        self.base.front_audio
    }

    pub fn back_audio_id(&self) -> Option<AudioId> {
        self.base.back_audio
    }

    pub fn card_type(&self) -> CType {
        self.base.data.fieldless()
    }

    pub fn dependents_ids(&self) -> BTreeSet<CardId> {
        self.card_provider
            .providers
            .cards
            .all_dependents(self.id)
            .into_iter()
            .collect()
    }

    pub fn dependents(&self) -> BTreeSet<Arc<Self>> {
        let mut cards = BTreeSet::default();
        for card in self.card_provider.dependents(self.id) {
            let card = self.card_provider.load(card).unwrap();
            cards.insert(card);
        }
        cards
    }

    pub fn add_review(&mut self, recall: Recall) {
        let action = ReviewAction::Insert(Review {
            grade: recall,
            timestamp: self.current_time(),
        });

        let event = ReviewEvent::new_modify(self.id, action);

        self.card_provider.providers.reviews.modify(event).unwrap();
        tracing::info!("added recall: {recall:?}");
    }

    pub fn time_provider(&self) -> FsTime {
        self.card_provider.time_provider()
    }

    pub fn lapses_last_month(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 30);

        self.history.lapses_since(day, current_time)
    }
    pub fn lapses_last_week(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400 * 7);

        self.history.lapses_since(day, current_time)
    }

    pub fn lapses_last_day(&self) -> u32 {
        let current_time = self.time_provider().current_time();
        let day = Duration::from_secs(86400);

        self.history.lapses_since(day, current_time)
    }

    pub fn from_parts(
        base: RawCard,
        history: History,
        metadata: Metadata,
        card_provider: CardProvider,
        recaller: SimpleRecall,
        front_audio: Option<Audio>,
        back_audio: Option<Audio>,
    ) -> Self {
        let id = base.id;

        let from_back =
            |back: &BackSide| -> EvalText { EvalText::from_backside(back, &card_provider, true) };

        let backside = match &base.data {
            CardType::Instance { back, class, .. } => match back.as_ref() {
                Some(back) => from_back(back),
                None => EvalText::just_some_ref(*class, &card_provider),
            },
            CardType::Normal { back, .. } => from_back(back),
            CardType::Unfinished { .. } => {
                EvalText::just_some_string("<unfinished>".to_string(), &card_provider)
            }
            CardType::Attribute { back, .. } => from_back(back),
            CardType::Class {
                back, parent_class, ..
            } => match (back, parent_class) {
                (Some(theback), Some(pcl)) if theback.is_empty_text() => {
                    EvalText::just_some_string(
                        card_provider
                            .providers
                            .cards
                            .load(*pcl)
                            .unwrap()
                            .data
                            .raw_front(),
                        &card_provider,
                    )
                }
                (None, Some(pcl)) => EvalText::just_some_ref(*pcl, &card_provider),
                (Some(back), _) => from_back(back),
                (_, _) => EvalText::default(),
            },
            CardType::Statement { .. } => {
                EvalText::just_some_string("<statement>".to_string(), &card_provider)
            }
            CardType::Event { .. } => {
                EvalText::just_some_string("<event>".to_string(), &card_provider)
            }
        };

        let mut frontside = base.data.display_front(&card_provider);

        if let Some(namespace) = base.namespace {
            let txt = TextLink::new(namespace);
            frontside
                .inner_mut()
                .insert(0, Either::Left("::".to_string()));
            frontside.inner_mut().insert(0, Either::Right(txt));
        }

        let frontside = EvalText::from_textdata(frontside, &card_provider);

        let name = EvalText::from_textdata(base.data.name(&card_provider), &card_provider);

        Self {
            namespace: base.namespace,
            id,
            frontside,
            base,
            name,
            backside,
            metadata,
            history,
            card_provider,
            recaller,
            front_audio,
            back_audio,
        }
    }

    pub fn namespace(&self) -> Option<CardId> {
        self.namespace
    }

    pub fn needs_work(&self) -> bool {
        self.metadata.needs_work
    }

    pub fn is_finished(&self) -> bool {
        !matches!(&self.base.data, CardType::Unfinished { .. })
    }

    /// which attribute cards describe this instance?
    pub fn attribute_cards(&self) -> HashSet<CardId> {
        if !self.is_instance() {
            dbg!(self);
            debug_assert!(false);
        }

        let getter = TheCacheGetter::ItemRef(RefGetter {
            reversed: true,
            key: self.id,
            ty: Some(CardRefType::InstanceOfAttribute),
            recursive: false,
        });

        self.card_provider.providers.cards.load_getter(getter)
    }

    pub fn is_attribute(&self) -> bool {
        matches!(&self.base.data, CardType::Attribute { .. })
    }

    pub fn is_instance(&self) -> bool {
        matches!(&self.base.data, CardType::Instance { .. })
    }

    pub fn is_class(&self) -> bool {
        matches!(&self.base.data, CardType::Class { .. })
    }

    pub fn is_instance_of(&self, _class: CardId) -> bool {
        if let CardType::Instance { .. } = &self.base.data {
            self.parent_classes().contains(&_class)
        } else {
            return false;
        }
    }

    pub async fn set_ref(mut self, reff: CardId) -> Result<Card, EventError<RawCard>> {
        let backside = BackSide::Card(reff);
        self.base = self.base.set_backside(backside);
        let action = CardAction::SetBackRef(reff);
        let event = CardEvent::new_modify(self.id, action);
        self.card_provider.providers.cards.modify(event).unwrap();
        Ok(self)
    }

    pub fn add_dependency(&mut self, dependency: CardId) -> Result<(), EventError<RawCard>> {
        self.base.explicit_dependencies.insert(dependency);
        let action = CardAction::AddDependency(dependency);
        let event = CardEvent::new_modify(self.id, action);
        self.card_provider.providers.cards.modify(event)
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        self.base.data.backside()
    }

    pub fn recursive_dependents(&self) -> HashSet<CardId> {
        self.card_provider.providers.cards.all_dependents(self.id)
    }

    pub fn recursive_dependencies(&self) -> Vec<CardId> {
        tracing::trace!("getting dependencies of: {}", self.id);
        let mut deps = vec![];
        let mut stack = vec![self.id()];

        while let Some(id) = stack.pop() {
            let Some(card) = self.card_provider.load(id) else {
                continue;
            };

            if self.id() != id {
                deps.push(id);
            }

            for dep in card.dependencies() {
                stack.push(dep);
            }
        }

        deps
    }

    pub fn min_rec_stability(&self) -> f32 {
        tracing::trace!("min rec recall of {}", self.id);
        let mut min_stability: RecallRate = f32::MAX;

        for card in self.recursive_dependencies() {
            let card = self.card_provider.load(card).unwrap();
            if !card.reviewable() {
                continue;
            } else {
                min_stability = min_stability.min(card.maturity_days().unwrap_or_default());
            }
        }

        min_stability
    }

    pub fn min_rec_recall_rate(&self) -> RecallRate {
        tracing::trace!("min rec recall of {}", self.id);
        let mut min_recall: RecallRate = 1.0;

        for card in self.recursive_dependencies() {
            let card = self.card_provider.load(card).unwrap();
            if !card.is_finished() {
                return 0.0;
            } else if !card.reviewable() {
                continue;
            } else {
                min_recall = min_recall.min(card.recall_rate().unwrap_or_default());
            }
        }

        min_recall
    }

    pub fn display_backside(&self) -> &str {
        &self.backside
    }

    pub fn back_refs(&self) -> Option<NonEmpty<CardId>> {
        match self.back_side() {
            Some(bs) => match bs {
                BackSide::Card(id) => Some(NonEmpty::from_vec(vec![*id]).unwrap()),
                BackSide::List(ids) => Some(NonEmpty::from_vec(ids.clone()).unwrap()),
                BackSide::Text(_) => None,
                BackSide::Time(_) => None,
                BackSide::Trivial => None,
                BackSide::Invalid => None,
                BackSide::Bool(_) => None,
            },
            None => None,
        }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    fn current_time(&self) -> Duration {
        self.card_provider.time_provider().current_time()
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        self.history.time_since_last_review(self.current_time())
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        SimpleRecall.recall_rate(&self.history, current_unix)
    }

    /// Full history includes all the successful reviews of cards that are dependent on this card.
    /// the idea is, if you can successfully recall a dependent card, then implicitly you know this card too.
    /// It does not include unsuccesful reviews of dependents because you may have failed to realize that card either due to the card itself or another dependency.
    pub fn full_history(&self) -> History {
        let mut reviews: Vec<Review> = vec![];
        for dep in self.dependents_ids() {
            let Some(history) = self.card_provider.providers.reviews.load(dep) else {
                continue;
            };

            for review in history.inner() {
                if review.is_success() {
                    reviews.push(review.to_owned());
                }
            }
        }

        reviews.sort_by_key(|r| r.timestamp);

        let mut history = self.history.clone();

        history.insert_many(reviews);
        history
    }

    pub fn full_recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller.recall_rate(&self.full_history(), now)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller.recall_rate(&self.history, now)
    }

    pub fn maturity_days(&self) -> Option<f32> {
        self.maturity().map(|d| d.as_secs_f32() / 86400.)
    }

    pub fn maturity(&self) -> Option<Duration> {
        use gkquad::single::integral;

        if self.recall_rate().is_none() {
            return None;
        }

        let now = self.current_time();
        let result = integral(
            |x: f64| {
                self.recall_rate_at(now + Duration::from_secs_f64(x * 86400.))
                    .unwrap_or_default() as f64
            },
            0.0..1000.,
        )
        .estimate()
        .ok()?;

        let dur = Duration::from_secs_f64(result * 86400.);

        Some(dur)
    }

    pub fn print(&self) -> String {
        self.frontside.to_string()
    }

    pub fn backside(&self) -> &EvalText {
        &self.backside
    }
    pub fn name(&self) -> &EvalText {
        &self.name
    }

    pub fn name_textdata(&self) -> TextData {
        self.base.data.name(&self.card_provider)
    }

    pub fn front_side(&self) -> &EvalText {
        &self.frontside
    }

    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.metadata.suspended.is_suspended()
    }

    pub fn set_suspend(&mut self, suspend: bool) {
        let action = TheLedgerAction::Modify(crate::ledger::MetaAction::Suspend(suspend));
        let event = MetaEvent::new(self.id, action);

        self.card_provider.providers.metadata.modify(event).unwrap();

        self.metadata.suspended = suspend.into();
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> CardId {
        self.id
    }

    pub fn explicit_dependencies(&self) -> HashSet<CardId> {
        self.base
            .explicit_dependencies
            .clone()
            .into_iter()
            .collect()
    }

    pub fn dependencies(&self) -> HashSet<CardId> {
        use ledgerstore::LedgerItem;

        self.base.dependencies()
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}
