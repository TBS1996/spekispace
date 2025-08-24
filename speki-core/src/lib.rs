use card::{CardId, RawCard};
use card_provider::CardProvider;
use dioxus_logger::tracing::info;
use ledgerstore::Ledger;
use ledgerstore::TimeProvider;
use metadata::Metadata;
use recall_rate::History;
use set::Set;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::path::PathBuf;
use std::{fmt::Debug, sync::Arc, time::Duration};
use textplots::Chart;
use textplots::Plot;
use textplots::Shape;
use tracing::trace;

pub mod audio;
pub mod card;
pub mod card_provider;
pub mod cardfilter;
pub mod collection;
mod common;
pub mod ledger;
pub mod metadata;
pub mod recall_rate;
pub mod set;

pub use card::{Card, CardType};
pub use common::current_time;
pub use omtrent::TimeStamp;
pub use recall_rate::SimpleRecall;

/// {from} is a(n) {ty} of {to}
#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardRefType {
    ExplicitDependency,
    ClassOfInstance,
    LinkRef,
    ParentClass,
    InstanceOfAttribute,
}

impl Display for CardRefType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for CardRefType {
    fn as_ref(&self) -> &str {
        match self {
            Self::ExplicitDependency => "explicit_dependency",
            Self::ClassOfInstance => "class_of_instance",
            Self::LinkRef => "linkref",
            Self::ParentClass => "parent_class",
            Self::InstanceOfAttribute => "instance_of_attribute",
        }
    }
}

use std::str::FromStr;

use crate::recall_rate::ml::Trained;
use crate::recall_rate::AvgRecall;
use crate::recall_rate::Recaller;
use crate::recall_rate::Review;

impl FromStr for CardRefType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "explicit_dependency" => Ok(Self::ExplicitDependency),
            "class_of_instance" => Ok(Self::ClassOfInstance),
            "linkref" => Ok(Self::LinkRef),
            "parent_class" => Ok(Self::ParentClass),
            "instance_of_attribute" => Ok(Self::InstanceOfAttribute),
            _ => Err(()),
        }
    }
}

impl CardRefType {
    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug)]
pub enum CardProperty {
    Trivial,
    Bigram,
    Suspended,
    CardType,
    AttrId,
    /// mapping of attributeid -> CardId
    Attr,
    Reviewable,
}

impl Display for CardProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for CardProperty {
    fn as_ref(&self) -> &str {
        match self {
            CardProperty::Bigram => "bigram",
            CardProperty::Suspended => "suspended",
            CardProperty::CardType => "cardtype",
            CardProperty::AttrId => "attr_id",
            CardProperty::Attr => "attr",
            CardProperty::Trivial => "trivial",
            CardProperty::Reviewable => "reviewable",
        }
    }
}

#[derive(Copy, Clone)]
pub struct FsTime;

impl TimeProvider for FsTime {
    fn current_time(&self) -> Duration {
        Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}

pub fn duplicates(provider: &CardProvider) -> HashSet<String> {
    info!("finding duplicates!");
    let mut cards: Vec<String> = provider
        .load_all()
        .into_iter()
        .map(|c| c.display_card(true, true).to_lowercase())
        .collect();

    cards.sort();

    let mut duplicates: HashSet<String> = Default::default();

    let mut prev = String::new();
    for card in cards.into_iter() {
        if &card == &prev {
            duplicates.insert(card.clone());
        }

        prev = card;
    }

    duplicates
}

pub fn current_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}

#[derive(Clone)]
pub struct Provider {
    pub cards: Ledger<RawCard>,
    pub sets: Ledger<Set>,
    pub reviews: Ledger<History>,
    pub metadata: Ledger<Metadata>,
    pub time: FsTime,
}

pub struct App {
    pub provider: Provider,
    pub card_provider: CardProvider,
    pub time_provider: FsTime,
    pub recaller: SimpleRecall,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

// helper: build log-spaced x values in DAYS (dense near "now")
fn log_spaced_days(start_min: f64, end_days: f64, step: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    let mut d = start_min / (24.0 * 60.0); // minutes -> days
    while d <= end_days {
        xs.push(d);
        d *= step;
    }
    xs
}

// Generic plotting helper: pass a function f(dt_days) -> probability
fn plot_recall<F>(title: &str, f: F, start: f64, end: f64)
where
    F: Fn(f64) -> f64,
{
    let xs = log_spaced_days(start, end, 1.15); // 1 minute .. ~10 years
    let pts: Vec<(f32, f32)> = xs.into_iter().map(|d| (d as f32, f(d) as f32)).collect();

    println!("\n{title}");

    Chart::new_with_y_range(360, 30, 0.0, end as f32, 0.0, 1.0)
        .lineplot(&Shape::Lines(&pts))
        .display();
}

pub fn plot_the_recall(card: Arc<Card>) {
    let recaller = Trained::from_static();
    let id = card.id();
    let reviews = card.history().reviews.clone();
    let now = reviews
        .first()
        .map(|x| x.timestamp)
        .unwrap_or(current_time());
    plot_card_recall_over_future(&recaller, id, &reviews, now);
    plot_card_recall_over_future(&SimpleRecall, id, &reviews, now);
    let avg = AvgRecall {
        trained: recaller,
        simple: SimpleRecall,
    };
    plot_card_recall_over_future(&avg, id, &reviews, now);
}

// Example glue — adapt names to your types:
fn plot_card_recall_over_future(
    recaller: &impl Recaller,
    card_id: CardId,
    seen_reviews: &[Review], // all past reviews for the card
    now: Duration,
) {
    plot_recall(
        "Recall vs time (future)",
        |dt_days| {
            let t = now + Duration::from_secs_f64(dt_days * 86_400.0);
            recaller.eval(card_id, seen_reviews, t).unwrap_or(0.0) as f64
        },
        1.0,
        30.,
    );
}

pub fn mean_rest(pairs: &[(f32, bool)]) -> f32 {
    let mut n = 0usize;
    let mut s = 0.0f32;
    for &(p, y) in pairs {
        if !p.is_finite() {
            continue;
        }
        let p = p.clamp(0.0, 1.0);
        s += if y { 1.0 - p } else { p };
        n += 1;
    }
    if n == 0 {
        return f32::NAN;
    }
    s / n as f32
}

struct PredEval {
    predicted: f64,
    recalled: bool,
    elapsed: Duration,
}

impl PredEval {
    fn log_loss(&self) -> f64 {
        let Self {
            predicted: p,
            recalled: y,
            elapsed,
        } = self;
        let _ = elapsed;
        let p = *p;
        let y = *y;

        let y = if y { 1.0 } else { 0.0 };
        let eps = 1e-15;
        if y == 1.0 {
            -(p.max(eps)).ln()
        } else {
            -(1.0 - p).max(eps).ln()
        }
    }

    fn log_loss_many(evals: Vec<Self>) -> f32 {
        let n = evals.len() as f64;
        if n == 0.0 {
            eprintln!("mean_error_accuracy: no valid predictions");
            return f32::NAN;
        }

        let mut logloss: f64 = 0.0;

        for pred in evals {
            logloss += pred.log_loss();
        }

        logloss /= n;

        logloss as f32
    }
}

fn get_pairs(history: &History, algo: &impl Recaller) -> (Vec<PredEval>, usize, usize) {
    let mut pairs = Vec::new();
    let mut seen = Vec::new();
    let mut bad = 0usize;
    let mut skipped = 0usize;
    let mut prev_ts: Option<Duration> = None;
    for r in &history.reviews {
        if !seen.is_empty() {
            if let Some(p) = algo.eval(history.id, &seen, r.timestamp) {
                if p.is_finite() {
                    pairs.push(PredEval {
                        predicted: p as f64,
                        recalled: r.is_success(),
                        elapsed: r.timestamp - prev_ts.unwrap(),
                    });
                } else {
                    bad += 1;
                }
            } else {
                skipped += 1;
            }
        }
        seen.push(r.clone());
        prev_ts = Some(r.timestamp);
    }

    (pairs, bad, skipped)
}

pub fn log_loss_accuracy(histories: &Vec<History>, algo: impl Recaller) -> f32 {
    let mut pairs = Vec::new();
    let mut bad = 0usize;
    let mut skipped = 0usize;

    for h in histories {
        let (_pairs, _bad, _skipped) = get_pairs(h, &algo);
        pairs.extend(_pairs);
        bad += _bad;
        skipped += _skipped;
    }

    println!("skipped: {skipped}");

    let n = pairs.len() as f64;
    if n == 0.0 {
        eprintln!("mean_error_accuracy: no valid predictions");
        return f32::NAN;
    }

    if bad > 0 {
        eprintln!("mean_error_accuracy: skipped {bad} non-finite predictions");
    }

    println!("skipped: {skipped}");

    PredEval::log_loss_many(pairs)
}

pub fn recall_algorithm_accuracy(ledger: &Ledger<History>) {
    let histories = ledger.load_all();

    let mut buckets: HashMap<u32, (u32, u32)> = Default::default();

    let recaller = Trained::from_static();

    for history in histories {
        for (rate, recalled) in history.rate_vs_result(recaller.clone()) {
            let bucket = ((rate * 10.0).floor() as u32).min(9);
            let entry = buckets.entry(bucket).or_default();
            if recalled {
                entry.0 += 1; // success
            } else {
                entry.1 += 1; // fail
            }
        }
    }

    let mut keys: Vec<_> = buckets.keys().cloned().collect();
    keys.sort();
    for bucket in keys {
        let (success, fail) = buckets[&bucket];
        let total = success + fail;
        let lower = bucket * 10;
        let upper = lower + 10;
        let acc = success as f32 / total as f32;
        println!("{lower}%-{upper}%: n={total}, success={acc:.2}");
    }
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        info!("initialtize app");

        let cards: Ledger<RawCard> = Ledger::new(root.clone());

        let provider = Provider {
            cards,
            reviews: Ledger::new(root.clone()),
            metadata: Ledger::new(root.clone()),
            time: FsTime,
            sets: Ledger::new(root),
        };

        let recaller = AvgRecall {
            trained: Trained::from_static(),
            simple: SimpleRecall,
        };
        let card_provider = CardProvider::new(provider.clone(), FsTime, recaller);

        Self {
            provider,
            card_provider,
            time_provider: FsTime,
            recaller: SimpleRecall,
        }
    }

    pub fn card_provider(&self) -> CardProvider {
        self.card_provider.clone()
    }

    pub async fn fill_cache(&self) {
        info!("filling cache");
        let start = self.time_provider.current_time();
        let elapsed = self.time_provider.current_time() - start;
        info!("cache filled in {:.4} seconds!", elapsed.as_secs_f32());
    }

    pub fn load_all_cards(&self) -> Vec<Arc<Card>> {
        self.card_provider.load_all()
    }

    pub fn load_card_sync(&self, id: CardId) -> Option<Card> {
        trace!("loading card: {id}");
        let card = self.card_provider.load(id);
        trace!("card loaded i guess: {card:?}");
        Some(Arc::unwrap_or_clone(card?))
    }

    pub fn load_card(&self, id: CardId) -> Option<Card> {
        self.load_card_sync(id)
    }

    pub async fn load_cards(&self) -> Vec<CardId> {
        self.card_provider.load_all_card_ids()
    }

    pub fn load_class_cards(&self) -> Vec<Arc<Card>> {
        self.load_all_cards()
            .into_iter()
            .filter(|card| card.is_class())
            .collect()
    }
}

pub fn as_graph(app: &App) -> String {
    graphviz::export(app)
}

pub mod graphviz {
    use std::collections::BTreeSet;

    use super::*;

    pub fn export_cards(cards: impl IntoIterator<Item = Arc<Card>>) -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();

        for card in cards {
            let label = card
                .print()
                .to_string()
                .replace(")", "")
                .replace("(", "")
                .replace("\"", "");

            let color = match card.recall_rate() {
                _ if !card.is_finished() => yellow_color(),
                Some(rate) => rate_to_color(rate as f64 * 100.),
                None => cyan_color(),
            };

            match card.recall_rate() {
                Some(rate) => {
                    let recall_rate = rate * 100.;
                    let maturity = card.maturity_days().unwrap_or_default();
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} ({:.0}%/{:.0}d)\", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        recall_rate,
                        maturity,
                        color
                    ));
                }
                None => {
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} \", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        color
                    ));
                }
            }

            // Create edges for dependencies, also enclosing IDs in quotes
            for child_id in card.dependencies() {
                relations.insert(format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        for rel in relations {
            dot.push_str(&rel);
        }

        dot.push_str("}\n");
        dot
    }

    pub fn export(app: &App) -> String {
        let cards = app.load_all_cards();
        export_cards(cards)
    }

    // Convert recall rate to a color, from red to green
    fn rate_to_color(rate: f64) -> String {
        let red = ((1.0 - rate / 100.0) * 255.0) as u8;
        let green = (rate / 100.0 * 255.0) as u8;
        format!("#{red:02X}{green:02X}00") // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}
