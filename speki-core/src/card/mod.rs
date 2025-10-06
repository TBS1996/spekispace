use core::f32;
use std::{
    cmp::{Ord, Ordering, PartialEq},
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    fmt::Debug,
    ops::Deref,
    sync::Arc,
    time::Duration,
};

pub mod basecard;
pub use basecard::*;

use either::Either;
use ledgerstore::{EventError, Leaf, LedgerAction, RefGetter, TimeProvider};
use nonempty::NonEmpty;
use serde::Deserializer;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    audio::{Audio, AudioId},
    card_provider::CardProvider,
    ledger::{CardAction, CardEvent, MetaEvent},
    metadata::Metadata,
    recall_rate::{History, Recall, Review, ReviewAction, ReviewEvent},
    ArcRecall, CardRefType, FsTime,
};

pub type RecallRate = f32;

#[derive(Clone, Debug, PartialEq)]
pub enum TextStyle {
    Faint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextComponent {
    pub data: Either<String, (String, CardId)>,
    pub style: Option<TextStyle>,
}

impl TextComponent {
    pub fn new_link(text: impl Into<String>, id: CardId) -> Self {
        Self {
            data: Either::Right((text.into(), id)),
            style: None,
        }
    }

    pub fn new_text(text: impl Into<String>) -> Self {
        Self {
            data: Either::Left(text.into()),
            style: None,
        }
    }

    pub fn with_style(self, style: TextStyle) -> Self {
        Self {
            style: Some(style),
            ..self
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvalText {
    cmps: Vec<TextComponent>,
    eval: String,
}

impl EvalText {
    pub fn components(&self) -> &Vec<TextComponent> {
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

    pub fn from_backside(b: &BackSide, provider: &CardProvider, hint: bool, name: bool) -> Self {
        match b {
            BackSide::Text(txt) => Self::from_textdata(txt.clone(), provider),
            BackSide::Card(id) => {
                let eval = if name {
                    provider.load(*id).unwrap().name.to_string()
                } else {
                    provider.load(*id).unwrap().frontside.to_string()
                };
                Self {
                    cmps: if hint {
                        vec![
                            TextComponent::new_text("🔗".to_string()),
                            TextComponent::new_link(eval.clone(), *id),
                        ]
                    } else {
                        vec![TextComponent::new_link(eval.clone(), *id)]
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
                Either::Left(s) => cmps.push(TextComponent::new_text(s)),
                Either::Right(TextLink { id, alias }) => match alias {
                    Some(alias) => {
                        cmps.push(TextComponent::new_link(alias, *id));
                    }
                    None => {
                        match provider.load(*id) {
                            Some(card) => {
                                let name = card.name.to_string();
                                cmps.push(TextComponent::new_link(name, *id));
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

pub struct DisplayData {
    data: Data,
    name: EvalText,
    namespace: Option<CardId>,
}

impl DisplayData {
    fn display(&self, provider: &CardProvider, with_namespace: bool, with_class: bool) -> EvalText {
        let mut text = TextData::default();

        // 1) Namespace (leftmost)
        if with_namespace {
            if let Some(ns) = self.namespace.as_ref() {
                text.push_link(*ns, None);
                text.push_string("::".to_string());
            }
        }

        match &self.data {
            // 2a) Attribute cards:  <Class>::instance[attribute]
            Data::Attribute { instance } => {
                if with_class {
                    if let Some(class_id) = provider.load(instance.1).unwrap().class() {
                        text.push_string("<".to_string());
                        text.push_link(class_id, None);
                        text.push_string(">::".to_string());
                    }
                }
                text.push_link(instance.1, None);
                text.push_string("[".to_string());
                text.push_eval(self.name.clone());
                text.push_string("]".to_string());
            }

            // 2b) Instance cards:   <Class>::instance
            Data::Instance { class, params } => {
                if with_class {
                    text.push_string("<".to_string());
                    text.push_link(class.1, None);
                    if !params.is_empty() {
                        text.push_string("<".to_string());
                        for (i, param) in params.iter().enumerate() {
                            if i > 0 {
                                text.push_string(", ".to_string());
                            }
                            text.push_string(param.1.to_string());
                        }
                        text.push_string(">".to_string());
                    }
                    text.push_string(">::".to_string());
                }
                // instance name (you had this in `self.name`)
                text.push_eval(self.name.clone());
            }

            // 2c) Class cards:      <Class> (optionally show parent as subtype)
            Data::Class { parent_class } => {
                if with_class {
                    text.push_string("<".to_string());
                    // class name is `self.name`
                    text.push_eval(self.name.clone());
                    text.push_string(">".to_string());

                    if let Some(parent) = parent_class {
                        text.push_string(" <: ".to_string());
                        text.push_link(parent.1, None);
                    }
                } else {
                    text.push_eval(self.name.clone());
                }
            }

            // 2d) Everything else: just render the name
            _ => {
                text.push_eval(self.name.clone());
            }
        }

        EvalText::from_textdata(text, provider)
    }
}

enum Data {
    Normal,
    Class {
        parent_class: Option<(String, CardId)>,
    },
    Instance {
        class: (String, CardId),
        params: Vec<(String, String)>,
    },
    Attribute {
        instance: (String, CardId),
    },
    Statement,
    Unfinished,
    Event,
    Invalid,
}

impl Data {}

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
    base: Arc<RawCard>,
    metadata: Arc<Metadata>,
    history: Arc<History>,
    card_provider: CardProvider,
    recaller: ArcRecall,
    is_remote: bool,
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
        write!(f, "{}", self.display_card(true, true).to_string())
    }
}

impl Card {
    pub fn trivial(&self) -> bool {
        self.base.trivial
    }

    pub fn ref_backside(&self) -> Option<&BackSide> {
        self.base.ref_backside()
    }

    pub fn transitive_sort(list: Vec<Self>) -> Result<Vec<Self>, ()> {
        let mut q: VecDeque<Self> = list.into_iter().collect();
        let mut out: Vec<Self> = Vec::with_capacity(q.len());

        let present: HashSet<CardId> = q.iter().map(|c| c.id()).collect();
        let mut placed: HashSet<CardId> = HashSet::with_capacity(q.len());

        let mut misses = 0usize;
        while let Some(card) = q.pop_front() {
            let ok = card
                .dependencies()
                .iter()
                .all(|d| !present.contains(d) || placed.contains(d));

            if ok {
                placed.insert(card.id());
                out.push(card);
                misses = 0;
            } else {
                q.push_back(card);
                misses += 1;
                if misses >= q.len() {
                    return Err(());
                }
            }
        }
        Ok(out)
    }

    pub fn metadata(&self) -> Metadata {
        Arc::unwrap_or_clone(self.metadata.clone())
    }

    pub fn display_card(&self, namespace: bool, class: bool) -> EvalText {
        self.displaying()
            .display(&self.card_provider, namespace, class)
    }

    fn displaying(&self) -> DisplayData {
        let card_provider = self.card_provider.clone();

        let data: Data = match &self.base.data {
            CardType::Instance {
                class: class_id,
                answered_params: _,
                ..
            } => {
                let class_name = card_provider.load(*class_id).unwrap().name().to_string();
                let mut params: Vec<(String, String)> = vec![];

                for (attr, ans) in self.param_to_ans() {
                    if let Some(ans) = ans {
                        let back =
                            EvalText::from_backside(&ans.answer, &card_provider, false, true)
                                .to_string();

                        params.push((attr.pattern, back));
                    }
                }

                params.sort();

                Data::Instance {
                    class: (class_name, *class_id),
                    params,
                }
            }
            CardType::Normal { .. } => Data::Normal,
            CardType::Unfinished { .. } => Data::Unfinished,
            CardType::Attribute { instance, .. } => {
                match card_provider.load(*instance).map(|x| x.name().to_owned()) {
                    Some(instance_name) => Data::Attribute {
                        instance: (instance_name.to_string(), *instance),
                    },
                    None => Data::Invalid,
                }
            }
            CardType::Class { parent_class, .. } => match parent_class {
                Some(parent) => match card_provider.load(*parent).map(|x| x.name().to_owned()) {
                    Some(parent_class_name) => Data::Class {
                        parent_class: Some((parent_class_name.to_string(), *parent)),
                    },
                    None => Data::Invalid,
                },
                None => Data::Class { parent_class: None },
            },
            CardType::Statement { .. } => Data::Statement,
            CardType::Event { .. } => Data::Event,
        };

        let namespace = self.namespace();

        DisplayData {
            data,
            name: self.name.clone(),
            namespace,
        }
    }

    pub fn param_to_ans(&self) -> BTreeMap<Attrv2, Option<ParamAnswer>> {
        self.base.data.param_to_ans(&self.card_provider)
    }

    pub fn params_on_parent_classes(&self) -> BTreeMap<CardId, Vec<Attrv2>> {
        let mut parents = self.parent_classes();
        parents.remove(&self.id());
        let mut out: BTreeMap<CardId, Vec<Attrv2>> = Default::default();

        for parent in parents {
            let params = self.card_provider.load(parent).unwrap().params_on_class();
            out.insert(parent, params);
        }

        out
    }

    pub fn recursive_params_on_class(&self) -> BTreeMap<CardId, Vec<Attrv2>> {
        let mut params = self.params_on_parent_classes();
        params.insert(self.id(), self.params_on_class());
        params
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

    pub fn clone_base(&self) -> Arc<RawCard> {
        self.base.clone()
    }

    pub fn attributes_on_class(&self) -> Option<Vec<Attrv2>> {
        if let CardType::Class { attrs, .. } = self.base.data.clone() {
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
            if let CardType::Class { attrs, .. } = &card.data {
                output.extend(attrs.clone());
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

        let getter = Leaf::Reference(RefGetter {
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

    pub fn direct_dependent_ids(&self) -> BTreeSet<CardId> {
        self.card_provider
            .providers
            .cards
            .dependents_direct(self.id)
            .into_iter()
            .collect()
    }

    pub fn recursive_dependent_ids(&self) -> BTreeSet<CardId> {
        self.card_provider
            .providers
            .cards
            .dependents_recursive(self.id)
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

    pub fn add_review(&self, recall: Recall) {
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

    pub fn is_remote(&self) -> bool {
        self.is_remote
    }

    pub fn from_parts(
        base: Arc<RawCard>,
        is_remote: bool,
        history: Arc<History>,
        metadata: Arc<Metadata>,
        card_provider: CardProvider,
        recaller: ArcRecall,
        front_audio: Option<Audio>,
        back_audio: Option<Audio>,
    ) -> Self {
        let id = base.id;

        let from_back = |back: &BackSide| -> EvalText {
            EvalText::from_backside(back, &card_provider, true, false)
        };

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
            is_remote,
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

        let getter = Leaf::Reference(RefGetter {
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

    pub fn add_dependency(&mut self, dependency: CardId) -> Result<(), EventError<RawCard>> {
        let action = CardAction::AddDependency(dependency);
        let event = CardEvent::new_modify(self.id, action);
        self.card_provider.providers.cards.modify(event)?;
        *self = Arc::unwrap_or_clone(self.card_provider.load(self.id).unwrap());
        Ok(())
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        self.base.data.backside()
    }

    pub fn recursive_dependents(&self) -> HashSet<CardId> {
        self.card_provider
            .providers
            .cards
            .dependents_recursive(self.id)
    }

    pub fn recursive_dependencies(&self) -> BTreeSet<Arc<Card>> {
        self.recursive_dependencies_ids()
            .into_iter()
            .map(|id| match self.card_provider.load(id) {
                Some(card) => card,
                None => {
                    dbg!("card not found", id);
                    panic!();
                }
            })
            .collect()
    }

    pub fn recursive_dependencies_ids(&self) -> HashSet<CardId> {
        self.card_provider
            .providers
            .cards
            .dependencies_recursive(self.id())
    }

    pub fn min_rec_stability(&self) -> f32 {
        tracing::trace!("min rec recall of {}", self.id);
        let mut min_stability: RecallRate = f32::MAX;

        for card in self.recursive_dependencies_ids() {
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

        for card in self.recursive_dependencies_ids() {
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
        self.recaller
            .eval(self.id, &self.history.reviews, current_unix)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = self.current_time();
        self.recaller
            .eval(self.id, &self.history.reviews, now)
            .map(|x| x as RecallRate)
    }

    pub fn maturity_days(&self) -> Option<f32> {
        self.maturity().map(|d| d.as_secs_f32() / 86400.)
    }

    pub fn maturity_inner_simple(
        id: CardId,
        time: Duration,
        reviews: &Vec<Review>,
        recall: &ArcRecall,
        sub_horizon: Duration,
    ) -> Option<Duration> {
        if reviews.is_empty() {
            return None;
        }

        // Tunables (unchanged)
        let step = Duration::from_secs(3600); // 1h step
        let horizon = Duration::from_secs(86_400 * 1000) - sub_horizon; // up to 1000 days
        let min_tail = Duration::from_secs(86_400 * 90); // no early-stop before 90 days
        let tiny_p = 1e-6; // “essentially zero”
        let quiet_steps = 48; // N consecutive tiny steps to stop

        #[inline]
        fn clamp01(x: f64) -> f64 {
            if x.is_finite() {
                x.clamp(0.0, 1.0)
            } else {
                0.0
            }
        }

        #[inline]
        fn p_at(id: CardId, t: Duration, reviews: &Vec<Review>, recall: &ArcRecall) -> f64 {
            clamp01(recall.eval(id, reviews, t).unwrap_or(0.0) as f64)
        }

        let area_sec =
            integrate_trapezoid(time, horizon, step, min_tail, quiet_steps, tiny_p, |t| {
                p_at(id, t, reviews, recall)
            });

        Some(Duration::from_secs_f64(area_sec))
    }

    pub fn maturity(&self) -> Option<Duration> {
        let now = self.current_time();
        Self::maturity_inner_simple(
            self.id,
            now,
            &self.history.reviews,
            &self.recaller,
            Default::default(),
        )
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
        let action = LedgerAction::Modify(crate::ledger::MetaAction::Suspend(suspend));
        let event = MetaEvent::new(self.id, action);

        self.card_provider.providers.metadata.modify(event).unwrap();
        *self = Arc::unwrap_or_clone(self.card_provider.load(self.id).unwrap());
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

pub fn integrate_trapezoid<F>(
    start: Duration,
    horizon: Duration,
    step: Duration,
    min_tail: Duration,
    quiet_steps: usize,
    tiny_threshold: f64,
    mut f: F,
) -> f64
where
    F: FnMut(Duration) -> f64,
{
    let mut elapsed = Duration::ZERO;
    let mut area_sec = 0.0;

    let mut prev = f(start);
    let mut quiet_run = 0usize;

    while elapsed < horizon {
        let remaining = (horizon - elapsed).min(step);
        let t_next = start + elapsed + remaining;
        let cur = f(t_next);

        let dt = remaining.as_secs_f64();
        area_sec += 0.5 * (prev + cur) * dt;

        if elapsed >= min_tail {
            if cur < tiny_threshold {
                quiet_run += 1;
            } else {
                quiet_run = 0;
            }
            if quiet_run >= quiet_steps {
                break;
            }
        }

        elapsed += remaining;
        prev = cur;
    }

    area_sec
}

#[cfg(test)]
mod tests {
    use super::integrate_trapezoid;
    use std::time::Duration;

    const EPS: f64 = 1e-9;

    fn dsecs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn constant_function_exact() {
        // f(t) = 2 over 100 s → area = 200
        let start = dsecs(0.0);
        let horizon = dsecs(100.0);
        let step = dsecs(1.0);
        let min_tail = Duration::ZERO;
        let quiet_steps = usize::MAX; // disable early-stop
        let tiny = -1.0; // irrelevant

        let area = integrate_trapezoid(start, horizon, step, min_tail, quiet_steps, tiny, |_| 2.0);
        assert!((area - 200.0).abs() < EPS, "area={}", area);
    }

    #[test]
    fn linear_function_exact() {
        // f(t) = a + b * (t - start), a=1, b=0.1 / s, horizon=10 s
        // ∫0..10 (1 + 0.1 x) dx = 10 + 0.05 * 100 = 15
        let start = dsecs(5.0);
        let horizon = dsecs(10.0);
        let step = dsecs(1.0);
        let area = integrate_trapezoid(
            start,
            horizon,
            step,
            Duration::ZERO,
            usize::MAX,
            -1.0,
            |t| {
                let x = (t - start).as_secs_f64();
                1.0 + 0.1 * x
            },
        );
        assert!((area - 15.0).abs() < 1e-9, "area={}", area);
    }

    #[test]
    fn quadratic_function_approx() {
        // f(t) = (t - start)^2 over 0..10 => ∫ x^2 dx = 10^3 / 3 = 333.333...
        // Trapezoid isn't exact, but with small step it's close.
        let start = dsecs(0.0);
        let horizon = dsecs(10.0);
        let step = dsecs(0.01);
        let exact = 1000.0f64 / 3.0; // 333.333...
        let area = integrate_trapezoid(
            start,
            horizon,
            step,
            Duration::ZERO,
            usize::MAX,
            -1.0,
            |t| {
                let x = (t - start).as_secs_f64();
                x * x
            },
        );
        assert!((area - exact).abs() < 1e-2, "area={} exact={}", area, exact);
    }

    #[test]
    fn early_stop_triggers() {
        // f(t) = 1 for first 5 seconds, then 0 afterwards.
        // Early-stop: after 3 consecutive values < 0.5, with min_tail=0.
        let start = dsecs(0.0);
        let horizon = dsecs(100.0);
        let step = dsecs(1.0);

        let area = integrate_trapezoid(start, horizon, step, Duration::ZERO, 3, 0.5, |t| {
            let x = (t - start).as_secs_f64();
            if x <= 5.0 {
                1.0
            } else {
                0.0
            }
        });

        // Without early-stop, area would be ~5.5 (last trapezoid across the 5→6 transition).
        // With quiet_steps=3, we will integrate a couple of extra zero segments, but not the whole horizon.
        assert!(area > 5.0 && area < 8.0, "area={}", area);
    }

    #[test]
    fn no_early_stop_if_min_tail_large() {
        // Same f as above, but min_tail prevents early-stop.
        let start = dsecs(0.0);
        let horizon = dsecs(10.0);
        let step = dsecs(1.0);

        let area = integrate_trapezoid(start, horizon, step, dsecs(1e9), 3, 0.5, |t| {
            let x = (t - start).as_secs_f64();
            if x <= 5.0 {
                1.0
            } else {
                0.0
            }
        });

        // We expect full trapezoid integration across the horizon:
        // segments [0..1]..[4..5] contribute 1 each, [5..6] contributes 0.5, rest are 0.
        // So area ≈ 5.5 exactly.
        assert!((area - 5.5).abs() < 1e-9, "area={}", area);
    }

    #[test]
    fn horizon_zero_is_zero() {
        let area = integrate_trapezoid(
            dsecs(0.0),
            dsecs(0.0),
            dsecs(1.0),
            Duration::ZERO,
            usize::MAX,
            -1.0,
            |_| 42.0,
        );
        assert!((area - 0.0).abs() < EPS, "area={}", area);
    }

    #[test]
    fn step_invariance_for_linear() {
        // Linear is exact for any step size
        let start = dsecs(0.0);
        let horizon = dsecs(13.0);
        let f = |t: Duration| 2.0 + 0.25 * t.as_secs_f64();

        let a1 = integrate_trapezoid(
            start,
            horizon,
            dsecs(1.0),
            Duration::ZERO,
            usize::MAX,
            -1.0,
            f,
        );
        let a2 = integrate_trapezoid(
            start,
            horizon,
            dsecs(0.5),
            Duration::ZERO,
            usize::MAX,
            -1.0,
            f,
        );
        let a3 = integrate_trapezoid(
            start,
            horizon,
            dsecs(0.2),
            Duration::ZERO,
            usize::MAX,
            -1.0,
            f,
        );

        assert!(
            (a1 - a2).abs() < 1e-9 && (a2 - a3).abs() < 1e-9,
            "a1={} a2={} a3={}",
            a1,
            a2,
            a3
        );
    }

    #[test]
    fn linear_non_divisible_horizon() {
        // ∫_0^10.5 (1 + 0.1x) dx = 16.0125
        let start = dsecs(0.0);
        let horizon = dsecs(10.5);
        let step = dsecs(1.0);
        let area = integrate_trapezoid(
            start,
            horizon,
            step,
            Duration::ZERO,
            usize::MAX,
            -1.0,
            |t| 1.0 + 0.1 * (t - start).as_secs_f64(),
        );
        assert!((area - 16.0125).abs() < 1e-9, "area={}", area);
    }

    #[test]
    fn step_larger_than_horizon() {
        // f(t)=2 over 0..0.75 -> area = 1.5
        let area = integrate_trapezoid(
            dsecs(0.0),
            dsecs(0.75),
            dsecs(1.0),
            Duration::ZERO,
            usize::MAX,
            -1.0,
            |_| 2.0,
        );
        assert!((area - 1.5).abs() < 1e-9, "area={}", area);
    }

    #[test]
    fn early_stop_threshold_strict() {
        // cur == tiny_threshold should NOT increment quiet_run
        let start = dsecs(0.0);
        let horizon = dsecs(5.0);
        let step = dsecs(1.0);
        let tiny = 0.5;
        let area = integrate_trapezoid(
            start,
            horizon,
            step,
            Duration::ZERO,
            1,
            tiny,
            |_t| tiny, // always equal to threshold
        );
        // No early stop; integrate full horizon: area = 0.5 * (0.5+0.5)*1 * 5 = 2.5
        assert!((area - 2.5).abs() < 1e-9, "area={}", area);
    }
}
