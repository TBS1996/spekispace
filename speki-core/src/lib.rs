use card::{CardId, RawCard};
use card_provider::CardProvider;
use dioxus_logger::tracing::info;
use indexmap::IndexSet;
use ledgerstore::CardChange;
use ledgerstore::EventError;
use ledgerstore::ItemAction;
use ledgerstore::Ledger;
use ledgerstore::Node;
use ledgerstore::TimeProvider;
use metadata::Metadata;
use nonempty::NonEmpty;
use recall_rate::History;
use serde::Deserialize;
use serde::Serialize;
use set::Set;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::{fmt::Debug, sync::Arc, time::Duration};
use textplots::Chart;
use textplots::Plot;
use textplots::Shape;
use tracing::trace;
use uuid::Uuid;

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

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecallChoice {
    Average,
    Simple,
    FSRS,
    #[default]
    Trained,
}

impl RecallChoice {
    pub fn get_instance(&self) -> ArcRecall {
        match self {
            RecallChoice::Average => Arc::new(Box::new(AvgRecall::default())),
            RecallChoice::Simple => Arc::new(Box::new(SimpleRecall)),
            RecallChoice::FSRS => Arc::new(Box::new(FSRS)),
            RecallChoice::Trained => Arc::new(Box::new(Trained::from_static())),
        }
    }
}

pub type ArcRecall = Arc<Box<dyn Recaller>>;

#[derive(Deserialize, Serialize, Debug)]
pub enum BackupStrategy {
    OnStart,
    Days(u32),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Backup {
    #[serde(default)]
    enable: bool,
    username: String,
    repo: String,
    branch: Option<String>, // default: main
    strategy: Option<BackupStrategy>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub randomize: bool,
    #[serde(default = "default_remote_github_username")]
    pub remote_github_username: String,
    #[serde(default = "default_remote_github_repo")]
    pub remote_github_repo: String,
    #[serde(default = "default_storage_path")]
    pub storage_path: PathBuf,
    #[serde(default)]
    pub recaller: RecallChoice,
    pub backup: Option<Backup>,
}

fn default_storage_path() -> PathBuf {
    dirs::data_local_dir().unwrap().join("speki")
}

fn default_remote_github_repo() -> String {
    "speki_graph".to_string()
}

fn default_remote_github_username() -> String {
    "tbs1996".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            randomize: false,
            remote_github_repo: default_remote_github_repo(),
            remote_github_username: default_remote_github_username(),
            storage_path: default_storage_path(),
            recaller: RecallChoice::default(),
            backup: None,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let dir = dirs::config_dir().unwrap().join("speki");
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    pub fn save_to_disk(&self) {
        use std::io::Write;
        let path = Self::path();
        let s = toml::to_string_pretty(self).unwrap();
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    pub fn load() -> Arc<Self> {
        let path = Self::path();

        if path.is_file() {
            let s = fs::read_to_string(&path).unwrap();
            match toml::from_str::<Self>(&s) {
                Ok(config) => Arc::new(config),
                Err(e) => {
                    dbg!(e);
                    Arc::new(Self::default())
                }
            }
        } else {
            Arc::new(Self::default())
        }
    }

    pub fn upstream_url() -> String {
        let config = Self::load();
        format!(
            "https://github.com/{}/{}",
            config.remote_github_username, config.remote_github_repo
        )
    }
}

/// {from} is a(n) {ty} of {to}
#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug, Deserialize, Serialize, Ord)]
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

use crate::cardfilter::CardFilter;
use crate::cardfilter::RecallState;
use crate::ledger::CardEvent;
use crate::ledger::Event;
use crate::ledger::MetaAction;
use crate::ledger::MetaEvent;
use crate::recall_rate::ml::classic::Trained;
use crate::recall_rate::AvgRecall;
use crate::recall_rate::Recall;
use crate::recall_rate::Recaller;
use crate::recall_rate::Review;
use crate::recall_rate::FSRS;
use crate::set::SetExpr;

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

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Debug, Serialize, Deserialize, Ord)]
pub enum CardProperty {
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

pub fn duplicates(provider: &CardProvider) -> IndexSet<String> {
    info!("finding duplicates!");
    let mut cards: Vec<String> = provider
        .load_all()
        .into_iter()
        .map(|c| c.display_card().to_lowercase())
        .collect();

    cards.sort();

    let mut duplicates: IndexSet<String> = Default::default();

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

pub struct ReviewableCards {
    pub seen: Vec<CardId>,
    pub unseen: Vec<CardId>,
}

pub fn reviewable_cards(
    provider: CardProvider,
    expr: SetExpr,
    filter: Option<CardFilter>,
    ordered: bool,
) -> Option<NonEmpty<CardId>> {
    let ReviewableCards { mut seen, unseen } =
        the_reviewable_cards(provider, expr, filter, ordered);
    seen.extend(unseen);

    NonEmpty::from_vec(seen)
}

/// Gets a list of cards to be reviewed based on the provided expression and filter.
pub fn the_reviewable_cards(
    provider: CardProvider,
    expr: SetExpr,
    mut filter: Option<CardFilter>,
    ordered: bool,
) -> ReviewableCards {
    info!("getting reviewable cards");
    let card_ids = provider.eval_expr(&expr);
    info!("{} cards loaded", card_ids.len());

    if ordered {
        filter.as_mut().map(|f| f.history.rec_stability = None);
        filter.as_mut().map(|f| f.history.lapses = None);
    }

    let mut nodes: Vec<Node<RawCard>> = Vec::with_capacity(card_ids.len());

    info!("start collecting nodes");
    for id in &card_ids {
        let node = provider.providers.cards.dependencies_recursive_node(*id);
        nodes.push(node);
    }
    info!("finished collecting nodes");

    let mut recalls: BTreeMap<CardId, RecallState> = Default::default();
    let hisledge = provider.providers.reviews.clone();
    let card_ledger = provider.providers.cards.clone();
    let time = current_time();

    info!("start eval nodes");
    let randomize = Config::load().randomize;
    for node in &nodes {
        RecallState::eval_card(
            node,
            &mut recalls,
            &hisledge,
            &card_ledger,
            time,
            provider.recaller.clone(),
            randomize,
        );
    }
    info!("finished eval nodes");

    let mut seen_cards: Vec<CardId> = vec![];
    let mut unseen_cards: Vec<CardId> = vec![];

    info!("start filter cards");

    for (idx, node) in nodes.into_iter().enumerate() {
        let id = node.id();
        let recstate = recalls.get(&id).unwrap();

        if !recstate.reviewable {
            continue;
        }

        let metadata = provider.load_metadata(id).map(|m| (*m).clone());
        let mut ordered_terminate = false;

        if filter
            .as_ref()
            .map(|filter| filter.filter(*recstate, metadata))
            .unwrap_or(true)
        {
            if recstate.pending {
                unseen_cards.push(id);
            } else {
                seen_cards.push(id);
            }
            ordered_terminate = true;
        };

        for dep in node.all_dependencies() {
            let dep_recstate = recalls.get(&dep).unwrap();
            if !dep_recstate.reviewable {
                continue;
            }

            if filter
                .as_ref()
                .map(|filter| {
                    filter.filter(
                        *dep_recstate,
                        provider.load_metadata(dep).map(|m| (*m).clone()),
                    )
                })
                .unwrap_or(true)
            {
                dbg!("adding dep");
                let depview = provider.load(dep).unwrap().display_card().to_string();
                dbg!(&depview);
                dbg!(dep_recstate);
                if dep_recstate.pending {
                    unseen_cards.push(dep);
                } else {
                    seen_cards.push(dep);
                }
                ordered_terminate = true;
            }
        }

        if ordered && ordered_terminate {
            let card_breaks = provider.load(id).unwrap().front_side().to_string();
            println!("breaking at node idx {idx}, card: {card_breaks}");
            break;
        }
    }

    info!("finish filter cards");

    use rand::prelude::SliceRandom;

    info!("start shuffle cards");
    seen_cards.shuffle(&mut rand::thread_rng());
    unseen_cards.shuffle(&mut rand::thread_rng());
    info!("finish shuffle cards");

    ReviewableCards {
        seen: seen_cards,
        unseen: unseen_cards,
    }
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
    pub recaller: ArcRecall,
}

impl Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app!")
    }
}

pub fn log_spaced(start: Duration, end: Duration, resolution: Duration) -> Vec<Duration> {
    assert!(end >= start, "end before start");
    let mut out = Vec::new();
    out.push(start);
    if end == start {
        return out;
    }

    let step = if resolution.is_zero() {
        Duration::from_nanos(1)
    } else {
        resolution
    };
    let mut t = start + step;

    while t <= end {
        out.push(t);
        t += step;
    }
    if *out.last().unwrap_or(&start) < end {
        out.push(end);
    }
    out
}

fn plot_recall<F>(
    title: &str,
    f: F,
    start: Duration,
    end: Duration,
    resolution: Duration,
    x_unit: Duration,
) where
    F: Fn(Duration) -> f64,
{
    let xs = log_spaced(start, end, resolution);

    let unit_secs = x_unit.as_secs_f64().max(1e-9);
    let xmax = (end - start).as_secs_f64() / unit_secs;

    let pts: Vec<(f32, f32)> = xs
        .into_iter()
        .map(|abs_t| {
            let x_units = (abs_t - start).as_secs_f64() / unit_secs;
            (x_units as f32, f(abs_t) as f32)
        })
        .collect();

    println!("\n{title}");
    Chart::new_with_y_range(1000, 60, 0.0, xmax as f32, 0.0, 1.0)
        .lineplot(&Shape::Lines(&pts))
        .display();
}

fn plot_card_recall_over_future(
    recaller: &impl Recaller,
    card_id: CardId,
    seen_reviews: &[Review],
) {
    let resolution = Duration::from_secs(3600);
    let x_unit = Duration::from_secs(3600);
    let start = seen_reviews.first().unwrap().timestamp;
    let horizon = Duration::from_secs(86400 * 10);

    plot_recall(
        "Recall vs time (future)",
        |abs_t| recaller.eval(card_id, seen_reviews, abs_t).unwrap_or(0.0) as f64,
        start,
        start + horizon,
        resolution,
        x_unit,
    );
}

pub fn plot_the_recall(card: Arc<Card>) {
    let recaller = Trained::from_static();
    let id = card.id();
    let reviews = card.history().reviews.clone();

    println!("ml");
    plot_card_recall_over_future(&recaller, id, &reviews);
    println!("simple");
    plot_card_recall_over_future(&SimpleRecall, id, &reviews);
    println!("fsrs");
    plot_card_recall_over_future(&FSRS, id, &reviews);

    let avg = AvgRecall::default();
    println!("avg");
    plot_card_recall_over_future(&avg, id, &reviews);
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

struct Point {
    maturity: Duration,
    succ_maturity: Duration,
    fail_maturity: Duration,
}

impl Point {
    fn expected_gain_days(&self, recall_rate: f32) -> f32 {
        let succ = self.succ_maturity.as_secs_f32();
        let fail = self.fail_maturity.as_secs_f32();

        let expected_maturity = succ * recall_rate + (1.0 - recall_rate) * fail;

        (expected_maturity - self.maturity.as_secs_f32()) / 86400.
    }
}

pub fn expected_gain(card: Arc<Card>, recaller: &ArcRecall) {
    let history = card.history().clone();
    let reviews = history.reviews;
    if reviews.is_empty() {
        println!("(no reviews)");
        return;
    }

    let id = card.id();
    let start = reviews.last().unwrap().timestamp + Duration::from_secs(1);

    // header
    println!("day  p_recall   Δsucc(d)   Δfail(d)   E[Δ](d)");
    println!("----------------------------------------------");

    let start_maturity =
        Card::maturity_inner_simple(id, start, &reviews, recaller, Duration::default()).unwrap();

    dbg!(start_maturity.as_secs_f32() / 86400.);

    for day in 0..=99 {
        if day % 5 != 0 {
            //continue;
        }
        let secs = 3600 * 24;
        // absolute timestamp (avoid cumulative-add bug)
        let time = start + Duration::from_secs(secs * day as u64);

        // probability of success at `time`
        let p = match recaller.eval(id, &reviews, time) {
            Some(p) if p.is_finite() => p as f64,
            _ => {
                println!("{:>3}    (skipped)", day);
                continue;
            }
        };

        let day_dur = Duration::from_secs(day * secs);

        // maturity if we do nothing until `time`
        let m_now = Card::maturity_inner_simple(id, time, &reviews, recaller, day_dur).unwrap();
        let mat_until = start_maturity - m_now;

        // simulate FAIL at `time`
        let mut failed_reviews = reviews.clone();
        failed_reviews.push(Review {
            timestamp: time,
            grade: Recall::Late,
        });
        let the_m_fail =
            Card::maturity_inner_simple(id, time, &failed_reviews, recaller, day_dur).unwrap();
        let m_fail = mat_until + the_m_fail;

        // simulate SUCCESS at `time` (use `Some` or `Perfect` per your semantics)
        let mut success_reviews = reviews.clone();
        success_reviews.push(Review {
            timestamp: time,
            grade: Recall::Some,
        });
        let the_m_succ =
            Card::maturity_inner_simple(id, time, &success_reviews, recaller, day_dur).unwrap();
        let m_succ = mat_until + the_m_succ;

        // signed deltas in SECONDS (Duration is unsigned, so do it in f64)
        let sec = |d: Duration| d.as_secs_f64();
        let d_succ_sec = sec(m_succ) - sec(start_maturity); // typically ≥ 0
        let d_fail_sec = sec(m_fail) - sec(start_maturity); // typically ≤ 0

        let point = Point {
            maturity: start_maturity,
            succ_maturity: m_succ,
            fail_maturity: m_fail,
        };

        let estimated = point.expected_gain_days(p as f32);

        // expected delta (in seconds), then convert to days for printing
        let _e_sec = p * d_succ_sec + (1.0 - p) * d_fail_sec;
        let to_days = |s: f32| s / 86_400.0;

        println!(
            "{:>3}   {:>7.3}   {:>7.3}   {:>7.3}   {:>7.3} {:>+7.3}",
            day,
            p,
            to_days(m_now.as_secs_f32()),
            to_days(m_succ.as_secs_f32()),
            to_days(m_fail.as_secs_f32()),
            estimated
        );

        //println!("succ: {}", m_succ.as_secs_f32() / 86400.);
    }
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

        let mut day_n = 0usize;
        let mut day_logloss = 0f64;
        let mut week_n = 0usize;
        let mut week_logloss = 0f64;
        let mut month_n = 0usize;
        let mut month_logloss = 0f64;
        let mut season_n = 0usize;
        let mut season_logloss = 0f64;
        let mut halfyear_n = 0usize;
        let mut halfyear_logloss = 0f64;
        let mut year_n = 0usize;
        let mut year_logloss = 0f64;
        let mut tail_n = 0usize;
        let mut tail_logloss = 0f64;

        for pred in evals {
            let loss = pred.log_loss();
            logloss += loss;

            if pred.elapsed < Duration::from_secs(86400) {
                day_n += 1;
                day_logloss += loss;
            } else if pred.elapsed < Duration::from_secs(86400 * 7) {
                week_n += 1;
                week_logloss += loss;
            } else if pred.elapsed < Duration::from_secs(86400 * 30) {
                month_n += 1;
                month_logloss += loss;
            } else if pred.elapsed < Duration::from_secs(86400 * 30 * 3) {
                season_n += 1;
                season_logloss += loss;
            } else if pred.elapsed < Duration::from_secs(86400 * 180) {
                halfyear_n += 1;
                halfyear_logloss += loss;
            } else if pred.elapsed < Duration::from_secs(86400 * 365) {
                year_n += 1;
                year_logloss += loss;
            } else {
                tail_n += 1;
                tail_logloss += loss;
            }
        }

        day_logloss /= day_n as f64;
        week_logloss /= week_n as f64;
        month_logloss /= month_n as f64;
        season_logloss /= season_n as f64;
        halfyear_logloss /= halfyear_n as f64;
        year_logloss /= year_n as f64;
        tail_logloss /= tail_n as f64;

        //dbg!(day_n, week_n, month_n, season_n, halfyear_n, year_n, tail_n);

        dbg!(
            day_logloss,
            week_logloss,
            month_logloss,
            season_logloss,
            halfyear_logloss,
            year_logloss,
            tail_logloss,
        );

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

#[derive(Debug)]
pub enum MyEventError {
    CardError(EventError<RawCard>),
    ReviewError(EventError<History>),
    MetaError(EventError<Metadata>),
}

impl From<EventError<RawCard>> for MyEventError {
    fn from(err: EventError<RawCard>) -> Self {
        MyEventError::CardError(err)
    }
}

impl From<EventError<History>> for MyEventError {
    fn from(err: EventError<History>) -> Self {
        MyEventError::ReviewError(err)
    }
}

impl From<EventError<Metadata>> for MyEventError {
    fn from(err: EventError<Metadata>) -> Self {
        MyEventError::MetaError(err)
    }
}

pub fn run_git_backup(path: &Path, cfg: &Backup) -> Result<(), String> {
    let repo_url = format!("https://github.com/{}/{}.git", cfg.username, cfg.repo);

    let path_str = path.to_str().ok_or_else(|| format!("Invalid path"))?;

    if !path.join(".git").exists() {
        println!("Initializing repo in {}", path_str);
        run(path, &["git", "init"])?;
        run(path, &["git", "remote", "add", "origin", &repo_url])?;
    } else {
        let output = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(path)
            .output()
            .unwrap();

        if output.status.success() {
            let current = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_ascii_lowercase();
            let repo_url = repo_url.to_ascii_lowercase();

            if current != repo_url {
                return Err(format!("origin mismatch: {} != {}", current, repo_url));
            }
        } else {
            run(path, &["git", "remote", "add", "origin", &repo_url])?;
        }
    }

    run(path, &["git", "add", "-A"])?;

    let status = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(path)
        .status()
        .unwrap();
    if !status.success() {
        run(path, &["git", "commit", "-m", "Speki backup"])?;
    }

    let branch = cfg.branch.as_deref().unwrap_or("main");

    run(path, &["git", "branch", "-M", branch])?;
    run(path, &["git", "push", "-u", "origin", branch])?;

    Ok(())
}

fn run(path: &Path, args: &[&str]) -> Result<(), String> {
    println!("$ {}", args.join(" "));
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(path)
        .status()
        .unwrap();
    if !status.success() {
        return Err(format!("command failed: {:?}", args));
    }
    Ok(())
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        info!("initialtize app");

        let config = Config::load();

        let recaller = config.recaller.get_instance();
        let cards: Ledger<RawCard> = Ledger::new(root.clone());

        if let Some(backup) = config.backup.as_ref() {
            if backup.enable {
                match backup.strategy {
                    Some(BackupStrategy::OnStart) => {
                        let path = config.storage_path.as_path();
                        let _ = dbg!(run_git_backup(path, backup));
                    }
                    Some(BackupStrategy::Days(_interval)) => {}
                    None => {}
                }
            }
        }

        let provider = Provider {
            cards,
            reviews: Ledger::new(root.clone()),
            metadata: Ledger::new(root.clone()),
            time: FsTime,
            sets: Ledger::new(root),
        };

        let card_provider = CardProvider::new(provider.clone(), FsTime, recaller.clone());

        Self {
            provider,
            card_provider,
            time_provider: FsTime,
            recaller,
        }
    }

    pub fn rebuild_card_state(&self) {
        self.provider.cards.apply();
    }

    pub fn apply_action(
        &self,
        action: ItemAction<RawCard>,
    ) -> Result<CardChange<RawCard>, EventError<RawCard>> {
        let res = self
            .card_provider
            .providers
            .cards
            .modify_actions(vec![action])?;

        assert!(res.len() == 1);
        Ok(res.into_iter().next().unwrap())
    }

    pub fn apply_many_actions(
        &self,
        events: Vec<ItemAction<RawCard>>,
    ) -> Result<Vec<CardChange<RawCard>>, EventError<RawCard>> {
        self.card_provider.providers.cards.modify_actions(events)
    }

    pub fn apply_many(&self, events: Vec<Event>) -> Result<(), MyEventError> {
        self.card_provider.many_modify(events)
    }

    pub fn review_cli(&self) {
        use std::io::{self, Write};
        use std::str::FromStr;

        let reviewable = the_reviewable_cards(
            self.card_provider.clone(),
            SetExpr::All,
            Some(CardFilter::default_filter()),
            false,
        );

        let cards = if !reviewable.seen.is_empty() {
            reviewable.seen
        } else if !reviewable.unseen.is_empty() {
            reviewable.unseen
        } else {
            println!("nothing to review!");
            return;
        };

        let qty = cards.len();

        for (idx, card_id) in cards.into_iter().enumerate() {
            let card = self.card_provider.load(card_id).unwrap();

            // FRONT
            print!("\x1b[2J\x1b[H"); // clear + home
            let front = card.display_card().to_string();
            println!("Card {}/{}\n", idx + 1, qty);
            println!("{front}");
            io::stdout().flush().ok();

            let mut buf = String::new();
            buf.clear();
            if io::stdin().read_line(&mut buf).is_err() {
                eprintln!("failed to read input");
                continue;
            }
            if buf.trim().eq_ignore_ascii_case("q") {
                break;
            }

            // BACK
            let back = card.display_backside();
            print!("\x1b[2J\x1b[H");
            println!("Card {}/{}\n", idx + 1, qty);
            println!("{front}");
            println!("\n---");
            println!("{back}\n");
            io::stdout().flush().ok();

            // PARSE RECALL (retry until valid or quit)
            let recall: Option<_> = loop {
                buf.clear();
                if io::stdin().read_line(&mut buf).is_err() {
                    eprintln!("failed to read input, try again:");
                    continue;
                }
                let t = buf.trim();
                if t.eq_ignore_ascii_case("q") {
                    return;
                } else if t.eq_ignore_ascii_case("d") {
                    match self.provider.cards.modify(CardEvent::new_delete(card_id)) {
                        Ok(_) => break None,
                        Err(e) => {
                            println!("{:?}", e);
                            continue;
                        }
                    }
                } else if t.eq_ignore_ascii_case("s") {
                    match self
                        .provider
                        .metadata
                        .modify(MetaEvent::new_modify(card_id, MetaAction::Suspend(true)))
                    {
                        Ok(_) => break None,
                        Err(e) => {
                            println!("{:?}", e);
                            continue;
                        }
                    }
                }
                match Recall::from_str(t) {
                    Ok(r) => break Some(r),
                    Err(_) => {
                        println!("couldn't parse '{t}'. try again (or 'q' to quit):");
                        io::stdout().flush().ok();
                    }
                }
            };

            if let Some(recall) = recall {
                card.add_review(recall);
            }
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
    use ledgerstore::{LedgerItem, ReadLedger};

    use super::*;

    use std::collections::{HashMap, HashSet};

    type NodeId = CardId;

    struct Group {
        id: Uuid,
        members: Vec<CardId>,
    }

    struct Node {
        id: NodeId,
        label: String,
        dependencies: Vec<CardId>,
        is_group: bool,
    }

    fn group_nodes(nodes: HashMap<NodeId, Node>) -> (HashMap<NodeId, Node>, bool) {
        const MAX_DEPENDENTS: usize = 50;

        // Step 1: Count how many nodes have each unique dependency SET
        let mut depmap: HashMap<Vec<CardId>, usize> = HashMap::new();
        for node in nodes.values() {
            *depmap.entry(node.dependencies.clone()).or_default() += 1;
        }

        // Step 2: Identify dependency sets that should be grouped
        let mut dep_sets_to_group: HashSet<Vec<CardId>> = HashSet::new();
        for (dep_set, count) in &depmap {
            // Don't group nodes with no dependencies (top-level nodes are independent)
            if !dep_set.is_empty() && *count > MAX_DEPENDENTS {
                dep_sets_to_group.insert(dep_set.clone());
            }
        }

        // If nothing to group, return unchanged
        if dep_sets_to_group.is_empty() {
            return (nodes, false);
        }

        // Step 3: Create groups (helper struct) and build groups_map
        let mut groups: HashMap<Vec<CardId>, Group> = HashMap::new();
        let mut groups_map: HashMap<NodeId, NodeId> = HashMap::new();

        for dep_set in &dep_sets_to_group {
            let group_id = uuid_from_hash(format!("{:?}", dep_set));
            let mut members = Vec::new();

            for node in nodes.values() {
                if &node.dependencies == dep_set {
                    members.push(node.id);
                    groups_map.insert(node.id.clone(), group_id);
                }
            }

            groups.insert(
                dep_set.clone(),
                Group {
                    id: group_id,
                    members,
                },
            );
        }

        // Step 4: Build new nodemap
        let mut new_nodes: HashMap<NodeId, Node> = HashMap::new();

        // Add group nodes (converted to regular Nodes)
        for (dep_set, group) in groups {
            // Remap group dependencies too (in case they point to grouped nodes)
            let mut remapped_deps: HashSet<CardId> = HashSet::new();
            for dep in &dep_set {
                if let Some(group_id) = groups_map.get(dep) {
                    remapped_deps.insert(*group_id);
                } else {
                    remapped_deps.insert(*dep);
                }
            }

            let mut deps_vec: Vec<CardId> = remapped_deps.into_iter().collect();
            deps_vec.sort();

            // Check if all dependencies are non-group nodes (for label formatting)
            let all_deps_are_nodes = deps_vec
                .iter()
                .all(|dep| nodes.get(dep).map(|n| !n.is_group).unwrap_or(false));

            let label = if all_deps_are_nodes && !deps_vec.is_empty() {
                let dep_labels: Vec<String> = deps_vec
                    .iter()
                    .filter_map(|dep| nodes.get(dep).map(|n| n.label.clone()))
                    .collect();
                if dep_labels.len() > 1 {
                    format!(
                        "{} cards with dependencies: {}",
                        group.members.len(),
                        dep_labels.join(", ")
                    )
                } else {
                    format!(
                        "{} cards with dependency: {}",
                        group.members.len(),
                        dep_labels.join(", ")
                    )
                }
            } else {
                format!("{} cards", group.members.len())
            };

            new_nodes.insert(
                group.id,
                Node {
                    id: group.id,
                    label,
                    dependencies: deps_vec,
                    is_group: true,
                },
            );
        }

        // Add ungrouped nodes with remapped dependencies
        for node in nodes.values() {
            if groups_map.contains_key(&node.id) {
                continue; // Skip nodes that got grouped
            }

            // Remap dependencies and deduplicate
            let mut remapped_deps: HashSet<CardId> = HashSet::new();
            for dep in &node.dependencies {
                if let Some(group_id) = groups_map.get(dep) {
                    // Dependency points to a grouped node, use group instead
                    remapped_deps.insert(*group_id);
                } else {
                    // Keep original dependency
                    remapped_deps.insert(*dep);
                }
            }

            let mut deps_vec: Vec<CardId> = remapped_deps.into_iter().collect();
            deps_vec.sort();

            new_nodes.insert(
                node.id,
                Node {
                    id: node.id,
                    label: node.label.clone(),
                    dependencies: deps_vec,
                    is_group: node.is_group,
                },
            );
        }

        (new_nodes, true)
    }

    fn create_dotgraph(nodes: &HashMap<NodeId, Node>) -> String {
        use indexmap::IndexSet;

        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations: IndexSet<String> = IndexSet::default();

        // Create nodes in the graph
        for node in nodes.values() {
            // Escape label for DOT format
            let label = node.label.replace("\"", "\\\"").replace("\n", "\\n");

            // Groups have "X cards" label format, regular nodes use their label
            let (shape, color) = if node.is_group {
                ("box", "#FFCCCC")
            } else {
                ("ellipse", "#CCDDFF")
            };

            dot.push_str(&format!(
                "    \"{}\" [label=\"{}\", shape={}, style=filled, fillcolor=\"{}\"];\n",
                node.id, label, shape, color
            ));
        }

        // Create edges (dependencies)
        for node in nodes.values() {
            for dep in &node.dependencies {
                relations.insert(format!("    \"{}\" -> \"{}\";\n", node.id, dep));
            }
        }

        // Add all relations to dot string
        for rel in relations {
            dot.push_str(&rel);
        }

        dot.push_str("}\n");
        dot
    }

    pub fn export_cards(ledger: &Ledger<RawCard>) -> String {
        // ALGORITHM (multi-pass to handle cascading groups):
        //
        // group_pass(nodes: HashMap<NodeId, Node>) -> (HashMap<NodeId, Node>, bool):
        //   1. Count how many nodes have each unique dependency SET
        //   2. For dependency sets with >MAX_DEPENDENTS occurrences, create groups:
        //      - group_id = UUID hashed from the dependency set
        //      - Build groups_map: grouped_node_id -> group_id
        //   3. Return new nodemap:
        //      - Group nodes (as regular Nodes): id=group_id, dependencies=shared dep set
        //      - Ungrouped nodes: dependencies remapped via groups_map (deduplicated)
        //   4. Return (new_nodemap, groups_were_created)
        //
        // Main loop:
        //   loop:
        //     (nodes, changed) = group_pass(nodes)
        //     if !changed: break
        //
        // This handles cascading: after german nouns group, their attribute cards
        // now share dependencies and can be grouped in the next pass.

        info!("exporting graphviz");
        let mut nodes: HashMap<NodeId, Node> = HashMap::new();

        info!("loading cards into nodes");
        for card_id in ledger.load_set_topologically_sorted(ledgerstore::ItemExpr::All) {
            let card = ledger.load(card_id).unwrap();
            let mut deps: Vec<_> = card.dependencies().into_iter().collect();
            deps.sort();
            let node = Node {
                id: card_id,
                label: card.frontside_eval(ledger).to_string(),
                dependencies: deps,
                is_group: false,
            };
            nodes.insert(card_id, node);
        }

        // Repeatedly group nodes until no more groups are created
        loop {
            info!("grouping pass");
            let (new_nodes, changed) = group_nodes(nodes);
            nodes = new_nodes;
            if !changed {
                info!("no more groups created, finishing");
                break;
            }
        }
        create_dotgraph(&nodes)
    }

    pub fn export(app: &App) -> String {
        export_cards(&app.provider.cards)
    }
}

/// Generate a UUID from the SHA-256 hash of the input data
///
/// Kinda hacky, since uuids are supposed to be random.
pub fn uuid_from_hash(input: impl AsRef<[u8]>) -> Uuid {
    let hash = Sha256::digest(input.as_ref());

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);

    // RFC 4122 variant
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    // Version 4 layout
    bytes[6] = (bytes[6] & 0x0f) | 0x40;

    Uuid::from_bytes(bytes)
}
