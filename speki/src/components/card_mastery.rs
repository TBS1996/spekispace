use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use dioxus::prelude::*;
use ledgerstore::TimeProvider;
use speki_core::recall_rate::{History as MyHistory, Recall};

use crate::{utils::recall_to_emoji, APP};

/// Info about the review history/recall/stability of the card
#[component]
pub fn MasterySection(history: MyHistory) -> Element {
    let now = APP.read().inner().time_provider.current_time();

    rsx! {
        DisplayHistory { history, now }
    }
}

fn hr_dur(dur: Duration) -> String {
    let secs = dur.as_secs_f32();
    if secs >= 86_400.0 {
        format!("{:.1}d ago", secs / 86_400.0)
    } else if secs >= 3_600.0 {
        format!("{:.1}h ago", secs / 3_600.0)
    } else if secs >= 60.0 {
        format!("{:.1}m ago", secs / 60.0)
    } else {
        format!("{:.0}s ago", secs)
    }
}

fn recall_to_bg_class(recall: Recall) -> &'static str {
    match recall {
        Recall::None => "bg-red-300",
        Recall::Late => "bg-red-200",
        Recall::Some => "bg-green-200",
        Recall::Perfect => "bg-green-300",
    }
}

#[component]
fn DisplayHistory(history: MyHistory, now: Duration, #[props(default = 5)] rows: usize) -> Element {
    let height_px = rows * 32;

    let reviews = history.reviews.clone();
    let is_empty = history.reviews.is_empty();

    let bg_emoji_ago_exact: Vec<(&str, &str, String, String)> = reviews
        .iter()
        .rev()
        .map(|review| {
            let bg = recall_to_bg_class(review.grade);
            let emoji = recall_to_emoji(review.grade);
            let ago = hr_dur(now - review.timestamp);

            let secs = review.timestamp.as_secs() as i64;
            let dt: DateTime<Local> = Local.timestamp_opt(secs, 0).unwrap();
            let exact = dt.format("%Y-%m-%d %H:%M:%S %Z").to_string();
            (bg, emoji, ago, exact)
        })
        .collect();

    rsx! {
        div {
            class: "space-y-2",
            h3 {
                class: "text-sm font-semibold text-gray-600 uppercase tracking-wide",
                "Past reviews"
            }
            div {
                class: "overflow-y-auto",
                style: "height: {height_px}px;",
                if is_empty {
                    p { "No review history." }
                } else {
                    for (bg, emoji, ago, exact) in bg_emoji_ago_exact {
                            div {
                                class: "rounded px-3 py-1 flex items-center justify-between text-base {bg}",
                                span {
                                    class: "text-2xl font-emoji leading-none",
                                    "{emoji}"
                                }
                                span { title: "{exact}", "{ago}" }
                            }
                    }
                }
            }
        }
    }
}
