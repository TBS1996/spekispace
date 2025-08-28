use std::{ops::Deref, time::Duration};

use ledgerstore::Ledger;

use crate::recall_rate::{History, Recall, Review};

fn squish(dt_days: f64) -> f64 {
    assert!(dt_days >= 0.);
    let eps_days = 1.0 / 86_400.0; // 1 second

    const A: &[(f64, f64)] = &[
        (0.0, 0.00),
        (1.0 / 1440.0, 0.03),
        (1.0 / 24.0, 0.10),
        (1.0, 0.20),
        (7.0, 0.30),
        (30.0, 0.40),
        (90.0, 0.50),
        (180.0, 0.70),
        (365.0, 0.80),
        (730.0, 0.90),
        (1000.0, 0.95),
        (3650.0, 0.99),
        (36500.0, 1.00),
    ];

    let mut logt = [0.0; A.len()];
    for (i, (d, _)) in A.iter().enumerate() {
        logt[i] = (d + eps_days).ln();
    }

    let x = (dt_days.max(0.0) + eps_days).ln();

    if x <= logt[0] {
        return A[0].1;
    }
    for i in 0..A.len() - 1 {
        if x <= logt[i + 1] {
            let y0 = A[i].1;
            let y1 = A[i + 1].1;
            let t = (x - logt[i]) / (logt[i + 1] - logt[i]);
            return (y0 + t * (y1 - y0)).clamp(0.0, 1.0);
        }
    }
    A[A.len() - 1].1
}

type Delta = f64;
type Grade = f64;

#[derive(Default)]
pub struct AllResults {
    one: Vec<RawObs>,
    two: Vec<RawObs>,
    three: Vec<RawObs>,
    four: Vec<RawObs>,
    five: Vec<RawObs>,
    six: Vec<RawObs>,
    seven: Vec<RawObs>,
}

impl AllResults {
    pub fn new(histories: &Vec<History>) -> Self {
        let mut obs: Vec<Observation<Classic>> = vec![];

        for history in histories {
            obs.extend(Observation::<Classic>::new_classic(history.clone()));
        }

        let mut selv = Self::default();

        for ob in obs {
            let raw = ob.into_raw();
            match bucket_k_from_len(raw.inputs.len()) {
                Some(1) => selv.one.push(raw),
                Some(2) => selv.two.push(raw),
                Some(3) => selv.three.push(raw),
                Some(4) => selv.four.push(raw),
                Some(5) => selv.five.push(raw),
                Some(6) => selv.six.push(raw),
                Some(7) => selv.seven.push(raw),
                _ => {} // ignore out of supported range
            }
        }

        selv
    }
}

fn bucket_k_from_len(len: usize) -> Option<usize> {
    if len % 2 != 0 {
        return None;
    }
    let k = len / 2; // number of reviews used
    if (1..=7).contains(&k) {
        Some(k)
    } else {
        None
    }
}

pub struct Observation<I: Into<RawInputs>> {
    inputs: I,
    recalled: bool,
}

impl<I: Into<RawInputs>> Observation<I> {
    fn into_raw(self) -> RawObs {
        RawObs {
            inputs: self.inputs.into(),
            output: (self.recalled as usize) as f64,
        }
    }

    fn new_classic(history: History) -> Vec<Observation<Classic>> {
        let mut out = vec![];
        let mut revs: Vec<Review> = vec![];

        for review in history.reviews {
            if let Some(inputs) = Classic::new(&revs, review.timestamp) {
                let recalled = review.is_success();
                out.push(Observation { inputs, recalled });
            }
            revs.push(review);
        }

        out
    }
}

#[derive(Debug, Clone)]
pub struct RawInputs(Vec<f64>);

impl Iterator for RawInputs {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl Deref for RawInputs {
    type Target = Vec<f64>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a RawInputs {
    type Item = &'a f64;
    type IntoIter = std::slice::Iter<'a, f64>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

const MAX_K: usize = 7;

pub struct RawObs {
    inputs: RawInputs,
    output: f64,
}
pub struct Classic {
    first: Grade,
    inner: Vec<(Delta, Grade)>,
    last: Delta,
}

fn recall_to_grade(recall: &Recall) -> Grade {
    match recall {
        Recall::None => 0.,
        Recall::Late => 0.1,
        Recall::Some => 0.9,
        Recall::Perfect => 1.0,
    }
}

impl From<Classic> for RawInputs {
    fn from(value: Classic) -> Self {
        let mut inputs: Vec<f64> = Vec::with_capacity(value.inner.len() * 2 + 2);
        inputs.push(value.first);
        for (delta, grade) in value.inner {
            inputs.push(delta);
            inputs.push(grade);
        }
        inputs.push(value.last);
        Self(inputs)
    }
}

impl Classic {
    pub fn new(reviews: &[Review], now: Duration) -> Option<Self> {
        if reviews.is_empty() {
            return None;
        }

        // Use only the last MAX_K reviews (tail cap)
        let used = if reviews.len() > MAX_K {
            &reviews[reviews.len() - MAX_K..]
        } else {
            reviews
        };

        // Last gap is from `now` to the timestamp of the *last used* review
        let last_gap_days = (now - used.last()?.timestamp).as_secs_f64() / 86_400.0;
        let last = squish(last_gap_days);

        // Build features from the *used* slice
        let mut it = used.iter();
        let first = it.next()?;
        let mut prev_ts = first.timestamp;

        let mut inner = Vec::with_capacity(used.len().saturating_sub(1));
        for Review { timestamp, grade } in it {
            let days = (*timestamp - prev_ts).as_secs_f64() / 86_400.0;
            inner.push((squish(days), recall_to_grade(grade)));
            prev_ts = *timestamp;
        }

        Some(Self {
            first: recall_to_grade(&first.grade),
            inner,
            last,
        })
    }
}

/// -------------------------------
/// Tiny logistic regression core.
/// -------------------------------
#[derive(Debug, Clone)]
pub struct Logistic {
    pub w: Vec<f64>,
    pub b: f64,
}

impl Logistic {
    pub fn predict_proba(&self, x: &RawInputs) -> f64 {
        debug_assert_eq!(x.len(), self.w.len());
        let z = self.w.iter().zip(x).map(|(w, xi)| w * xi).sum::<f64>() + self.b;
        sigmoid(z)
    }
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        let ez = (-z).exp();
        1.0 / (1.0 + ez)
    } else {
        let ez = z.exp();
        ez / (1.0 + ez)
    }
}

/// Train one logistic model on a slice of RawObs (all with same input length).
/// - lr: learning rate (try 0.1 .. 0.5)
/// - epochs: 200..2000
/// - l2: small L2 like 1e-4
pub fn train_bucket(obs: &[RawObs], lr: f64, epochs: usize, l2: f64) -> Option<Logistic> {
    if obs.is_empty() {
        return None;
    }
    let d = obs[0].inputs.len();
    if d == 0 {
        return None;
    }
    // Sanity: all same dimension
    debug_assert!(obs.iter().all(|o| o.inputs.len() == d));

    let mut w = vec![0.0; d];
    let mut b = 0.0;
    let n = obs.len() as f64;

    for _ in 0..epochs {
        let mut gw = vec![0.0; d];
        let mut gb = 0.0;

        for o in obs {
            let y = o.output; // assumed 0.0 or 1.0
            let p = {
                let z = w
                    .iter()
                    .zip(&o.inputs.0)
                    .map(|(wi, xi)| wi * xi)
                    .sum::<f64>()
                    + b;
                sigmoid(z)
            };
            let err = p - y; // d(-loglik)/dz

            for i in 0..d {
                gw[i] += err * o.inputs[i];
            }
            gb += err;
        }

        // Average + L2 on weights (not bias)
        for i in 0..d {
            gw[i] = gw[i] / n + l2 * w[i];
        }
        gb /= n;

        // Gradient step
        for i in 0..d {
            w[i] -= lr * gw[i];
        }
        b -= lr * gb;
    }

    Some(Logistic { w, b })
}

/// -------------------------------
/// Train per-bucket from AllResults
/// -------------------------------
#[derive(Debug, Default, Clone)]
pub struct Trained {
    pub one: Option<Logistic>,
    pub two: Option<Logistic>,
    pub three: Option<Logistic>,
    pub four: Option<Logistic>,
    pub five: Option<Logistic>,
    pub six: Option<Logistic>,
    pub seven: Option<Logistic>,
}

pub fn train_all(all: &AllResults, lr: f64, epochs: usize, l2: f64) -> Trained {
    Trained {
        one: train_bucket(&all.one, lr, epochs, l2),
        two: train_bucket(&all.two, lr, epochs, l2),
        three: train_bucket(&all.three, lr, epochs, l2),
        four: train_bucket(&all.four, lr, epochs, l2),
        five: train_bucket(&all.five, lr, epochs, l2),
        six: train_bucket(&all.six, lr, epochs, l2),
        seven: train_bucket(&all.seven, lr, epochs, l2),
    }
}

/// Route a feature vector to the right bucket model by its length.
/// Returns None if there is no trained model for that length.
impl Trained {
    pub fn from_static() -> Self {
        Trained {
            one: Some(Logistic {
                w: vec![3.3212436779979773, -0.5862314892336042],
                b: -0.9978228823016541,
            }),
            two: Some(Logistic {
                w: vec![
                    1.672688768578726,
                    -0.26499921840844676,
                    1.9989576192759584,
                    -0.5445922112613977,
                ],
                b: -0.9909675852817269,
            }),
            three: Some(Logistic {
                w: vec![
                    1.1234356279375757,
                    -0.15843054282428817,
                    1.222091379637708,
                    -0.19990760196863058,
                    2.640193243309847,
                    -0.566862071160717,
                ],
                b: -1.7296714509328888,
            }),
            four: Some(Logistic {
                w: vec![
                    0.8571148457713729,
                    -0.009663972904993951,
                    0.8009576715536639,
                    -0.15340584092952314,
                    1.0171322334224706,
                    0.05726193604693113,
                    1.6832019759262222,
                    -0.8043403794047786,
                ],
                b: -1.353877303565703,
            }),
            five: Some(Logistic {
                w: vec![
                    0.6690732974019291,
                    0.05337749818517778,
                    0.5089180421649346,
                    -0.31832303279919066,
                    0.7977593461533967,
                    0.08065737256431946,
                    0.8102126728834369,
                    -0.35732641670272,
                    2.81662868247357,
                    -0.4298302519808251,
                ],
                b: -2.0324660379813944,
            }),
            six: Some(Logistic {
                w: vec![
                    0.44377532836662786,
                    -0.06281315848049829,
                    0.858711766487895,
                    0.011456755887358798,
                    0.6633423796422627,
                    0.14978887962756693,
                    0.5537455571156519,
                    -0.09295089364903303,
                    0.8188361615105705,
                    -0.01712695539784132,
                    1.1935347739300037,
                    -0.7418881190444127,
                ],
                b: -1.5915435367915578,
            }),
            seven: Some(Logistic {
                w: vec![
                    0.44522527409137336,
                    0.2428412955549915,
                    0.28984801849752806,
                    0.19041598453051242,
                    0.34255275995193313,
                    0.3333609966505993,
                    0.40006253144742643,
                    0.18377555198672968,
                    0.6311411266729162,
                    0.18430999093125455,
                    0.5128221531536855,
                    -0.22988014650569552,
                    1.7471510798052492,
                    -0.6840092152714291,
                ],
                b: -1.9070341288933403,
            }),
        }
    }

    pub fn new(histories: &Vec<History>) -> Self {
        let res = AllResults::new(histories);
        train_all(&res, 0.2, 800, 1e-4)
    }

    pub fn recall_rate(&self, history: &[Review], current_time: Duration) -> Option<f64> {
        let mut reviews: Vec<Review> = vec![];
        for review in history {
            if review.timestamp < current_time {
                reviews.push(review.clone());
            }
        }
        let inputs = Self::current_inputs(&reviews, current_time)?;
        self.predict_proba(&inputs)
    }

    fn current_inputs(history: &[Review], now: Duration) -> Option<RawInputs> {
        let inputs: Classic = Classic::new(history, now)?;
        Some(inputs.into())
    }

    fn predict_proba(&self, x: &RawInputs) -> Option<f64> {
        match bucket_k_from_len(x.len()) {
            Some(1) => self.one.as_ref().map(|m| m.predict_proba(x)),
            Some(2) => self.two.as_ref().map(|m| m.predict_proba(x)),
            Some(3) => self.three.as_ref().map(|m| m.predict_proba(x)),
            Some(4) => self.four.as_ref().map(|m| m.predict_proba(x)),
            Some(5) => self.five.as_ref().map(|m| m.predict_proba(x)),
            Some(6) => self.six.as_ref().map(|m| m.predict_proba(x)),
            Some(7) => self.seven.as_ref().map(|m| m.predict_proba(x)),
            _ => None,
        }
    }
}

pub fn eval_per_bucket(ledger: &Ledger<History>, trained: &Trained) {
    let mut buckets: Vec<Vec<(f64, bool)>> = vec![vec![]; 8]; // 1..=7
    for h in ledger.load_all() {
        let mut seen = Vec::new();
        for r in &h.reviews {
            if !seen.is_empty() {
                if let Some(inputs) = Classic::new(&seen, r.timestamp) {
                    let raw = inputs.into();
                    if let Some(p) = trained.predict_proba(&raw) {
                        if p.is_finite() {
                            let y = r.is_success();
                            if let Some(k) = bucket_k_from_len(raw.len()) {
                                buckets[k].push((p, y));
                            }
                        }
                    }
                }
            }
            seen.push(r.clone());
        }
    }

    for k in 1..=7 {
        let data = &buckets[k];
        if data.is_empty() {
            println!("k={k}: no data");
            continue;
        }
        let n = data.len() as f64;
        let mut mae = 0.0;
        let mut brier = 0.0;
        let mut logloss = 0.0;
        for &(p, yb) in data {
            let y = if yb { 1.0 } else { 0.0 };
            mae += (p - y).abs();
            brier += (p - y).powi(2);
            let eps = 1e-15;
            if y == 1.0 {
                logloss += -(p.max(eps)).ln();
            } else {
                logloss += -(1.0 - p).max(eps).ln();
            }
        }
        println!(
            "k={k}  n={}  MAE={:.3}  Brier={:.3}  LogLoss={:.3}",
            data.len(),
            mae / n,
            brier / n,
            logloss / n
        );
    }
}
