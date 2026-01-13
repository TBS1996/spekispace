use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use dioxus::prelude::*;
use speki_core::recall_rate::{History as MyHistory, Recall, ReviewAction, ReviewEvent};

use crate::{utils::recall_to_emoji, APP};

/// Info about the review history/recall/stability of the card
#[component]
pub fn MasterySection(history: MyHistory, card_id: speki_core::card::CardId) -> Element {
    let now = APP.read().current_time();

    rsx! {
        DisplayHistory { history, now, card_id }
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
fn DisplayHistory(
    history: MyHistory,
    now: Duration,
    card_id: speki_core::card::CardId,
    #[props(default = 5)] rows: usize,
) -> Element {
    let height_px = rows * 32;

    let reviews = history.reviews.clone();
    let is_empty = history.reviews.is_empty();

    let bg_emoji_ago_exact_ts: Vec<(&str, &str, String, String, Duration)> = reviews
        .iter()
        .rev()
        .map(|review| {
            let bg = recall_to_bg_class(review.grade);
            let emoji = recall_to_emoji(review.grade);
            let ago = hr_dur(now - review.timestamp);

            let secs = review.timestamp.as_secs() as i64;
            let dt: DateTime<Local> = Local.timestamp_opt(secs, 0).unwrap();
            let exact = dt.format("%Y-%m-%d %H:%M:%S %Z").to_string();
            (bg, emoji, ago, exact, review.timestamp)
        })
        .collect();

    // Calculate recall and stability
    let recall_pct = history.recall_rate(now).map(|r| (r * 100.0) as i32);
    let stability_days = history.maturity_days(now).map(|d| d as i32);

    rsx! {
        div {
            class: "space-y-2",

            // Display recall and stability
            div {
                class: "flex gap-3 text-xs mb-1",
                if let Some(recall) = recall_pct {
                    div {
                        span { class: "text-gray-500", "Recall: " }
                        span { class: "font-semibold", "{recall}%" }
                    }
                }
                if let Some(stab) = stability_days {
                    div {
                        span { class: "text-gray-500", "Stability: " }
                        span { class: "font-semibold", "{stab} days" }
                    }
                }
            }

            div {
                class: "overflow-y-auto pr-2",
                style: "height: {height_px}px;",
                if is_empty {
                    p { "No review history." }
                } else {
                    for (bg, emoji, ago, exact, timestamp) in bg_emoji_ago_exact_ts {
                            div {
                                class: "rounded px-3 py-1 flex items-center gap-2 text-base {bg}",
                                span {
                                    class: "text-2xl font-emoji leading-none",
                                    "{emoji}"
                                }
                                div {
                                    class: "flex items-center gap-1 flex-1 justify-end",
                                    span { title: "{exact}", "{ago}" }
                                    button {
                                        class: "text-gray-600 hover:text-red-600 font-bold text-lg leading-none",
                                        title: "Delete this review",
                                        onclick: move |_| {
                                            let event = ReviewEvent::new_modify(card_id, ReviewAction::Remove(timestamp));
                                            let _ = APP.read().modify_history(event);
                                        },
                                        "\u{d7}"
                                    }
                                }
                            }
                    }
                }
            }
        }
    }
}
