use std::sync::Arc;

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, collection::Collection};

use crate::{
    components::{FilterComp, FilterEditor},
    overlays::{colviewer::ColViewer, reviewsession::ReviewState, textinput::TextInput},
    APP, IS_SHORT, OVERLAY,
};

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    cardfilter: Memo<CardFilter>,
    collections: Signal<Vec<(Collection, RecallDist)>>,
}

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();
        let selv = Self {
            filter,
            cardfilter,
            collections: Default::default(),
        };

        let cols = selv.collections.clone();

        spawn(async move {
            let _cols = APP.read().load_collections().await;
            let mut out = vec![];

            for col in _cols {
                let dist = RecallDist::new(col.clone()).await;
                out.push((col, dist));
            }
            cols.clone().set(out);
        });

        selv
    }
}

#[component]
pub fn Review() -> Element {
    let state: ReviewPage = use_context::<ReviewPage>();
    let editor = state.filter.clone();
    tracing::info!("memo lol: {:?}", &state.cardfilter);

    let class = if IS_SHORT.cloned() {
        "flex flex-col items-center h-screen space-y-4 justify-center"
    } else {
        "flex flex-col items-start h-screen space-y-4 pl-32"
    };

    rsx! {
        div {
            class: "{class}",

            div {
                class: "flex space-x-4 mt-6",

                { render_collections(state) }


                 FilterComp {editor}

            }
        }
    }
}

#[component]
fn RecallBar(dist: RecallDist) -> Element {
    let proportions = dist.proportions();

    rsx!(
        div {
            class: "flex w-full h-4 rounded overflow-hidden border border-gray-300",
            for (percentage, color) in proportions {
                div {
                    style: format!(
                        "width: {}%; background-color: {};",
                        percentage,
                        color
                    ),
                    key: "{color}",
                }
            }
        }
    )
}

#[derive(Default, Clone, PartialEq, Debug)]
struct RecallDist {
    p: u32,
    n1: u32,
    n2: u32,
    n3: u32,
    n4: u32,
    n5: u32,
    n6: u32,
}

impl RecallDist {
    const HEXP: &str = "#00FFFF";
    const HEX1: &str = "#FF0D0D";
    const HEX2: &str = "#FF4E11";
    const HEX3: &str = "#FF8E15";
    const HEX4: &str = "#FAB733";
    const HEX5: &str = "#ACB334";
    const HEX6: &str = "#69B34C";

    fn total(&self) -> u32 {
        self.p + self.n1 + self.n2 + self.n3 + self.n4 + self.n5 + self.n6
    }

    fn proportions(&self) -> Vec<(f32, &'static str)> {
        let total = self.total();
        if total == 0 {
            return vec![];
        }
        vec![
            (self.p as f32 / total as f32 * 100.0, Self::HEXP),
            (self.n1 as f32 / total as f32 * 100.0, Self::HEX1),
            (self.n2 as f32 / total as f32 * 100.0, Self::HEX2),
            (self.n3 as f32 / total as f32 * 100.0, Self::HEX3),
            (self.n4 as f32 / total as f32 * 100.0, Self::HEX4),
            (self.n5 as f32 / total as f32 * 100.0, Self::HEX5),
            (self.n6 as f32 / total as f32 * 100.0, Self::HEX6),
        ]
    }

    async fn new(col: Collection) -> Self {
        let mut selv = Self::default();

        for card in col
            .expand(APP.read().inner().card_provider(), Default::default())
            .await
        {
            *match card.recall_rate() {
                Some(rate) => {
                    if rate < 0.05 {
                        &mut selv.n1
                    } else if rate < 0.2 {
                        &mut selv.n2
                    } else if rate < 0.5 {
                        &mut selv.n3
                    } else if rate < 0.8 {
                        &mut selv.n4
                    } else if rate < 0.95 {
                        &mut selv.n5
                    } else {
                        &mut selv.n6
                    }
                }
                None => &mut selv.p,
            } += 1;
        }

        tracing::info!("{selv:?}");

        selv
    }
}

fn render_collections(state: ReviewPage) -> Element {
    let filter = state.filter.to_filter();
    let collections = state.collections.clone();

    let mut colfil: Vec<(Collection, RecallDist, CardFilter)> = vec![];

    for (col, dist) in collections.cloned() {
        colfil.push((col, dist, filter.clone()));
    }

    rsx! {
        div {
            class: "flex flex-col max-w-[550px] mr-5",

            button {
                class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-8",
                onclick: move |_| {
                    let filter = filter.clone();
                    spawn(async move {
                        let cards = APP.read().load_all(Some(filter)).await;
                        let session = ReviewState::new(cards).await;
                        OVERLAY.cloned().set(Box::new(session));
                    });
                },
                "review all"

            }

            for (col, dist, filter) in colfil {
                div {
                    class: "flex flex-col mb-8",
                    div {
                    class: "flex flex-row",
                        button {
                            onclick: move |_|{
                                spawn(async move {
                                    let viewer = ColViewer::new(col.id).await;
                                    OVERLAY.read().set(Box::new(viewer));
                                });
                            },
                            "✏️"
                        }
                        button {
                            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                            onclick: move |_| {
                                let filter = filter.clone();
                                spawn(async move {
                                    let col = APP.read().load_collection(col.id).await;
                                    let cards = col.expand(APP.read().inner().card_provider.clone(), Default::default()).await;
                                    let session = ReviewState::new_with_filter(cards, filter).await;
                                    OVERLAY.cloned().set(Box::new(session));
                                });
                            },
                            "{col.name}"
                        }
                    }

                    RecallBar { dist  }
                }
            }

            button {
                class: "inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                onclick: move |_| {
                    let f = move |name: String| {
                        let col = Collection::new(name);
                        spawn(async move {
                            APP.read().save_collection(col).await;
                        });
                    };

                    let txt = TextInput::new("add collection".to_string(), Arc::new(Box::new(f)));
                    OVERLAY.read().set(Box::new(txt));
                },
                "add collection"
            }


        }
    }
}
