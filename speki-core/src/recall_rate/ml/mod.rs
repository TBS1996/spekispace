pub mod classic;

use std::ops::Deref;

use crate::recall_rate::{ml::classic::Classic, History, Review};

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

/// Observation is used for training data
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

pub struct RawObs {
    inputs: RawInputs,
    output: f64,
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
