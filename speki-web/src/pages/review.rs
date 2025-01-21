use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, collection::Collection};

use crate::{
    components::{FilterComp, FilterEditor},
    overlays::{
        card_selector::CardSelector,
        cardviewer::CardViewer,
        colviewer::ColViewer,
        itemselector::ItemSelector,
        reviewsession::{ReviewRender, ReviewState},
        textinput::TextInput,
        uploader::Uploader,
        yesno::Yesno,
    },
    APP, IS_SHORT,
};

use crate::components::Komponent;
use crate::overlays::Overlay;

#[derive(Clone)]
pub enum OverlayEnum {
    Review(ReviewState),
    Colviewer(ColViewer),
    Text(TextInput),
    CardViewer(CardViewer),
    CardSelector(CardSelector),
    YesNo(Yesno),
    ColSelector(ItemSelector<Collection>),
    Uploader(Uploader),
}

impl OverlayEnum {
    pub fn is_done(&self) -> bool {
        match self {
            OverlayEnum::Review(elm) => elm.is_done.cloned(),
            OverlayEnum::Colviewer(elm) => elm.is_done().cloned(),
            OverlayEnum::Text(elm) => elm.is_done().cloned(),
            OverlayEnum::CardViewer(elm) => elm.is_done().cloned(),
            OverlayEnum::CardSelector(elm) => elm.is_done().cloned(),
            OverlayEnum::YesNo(elm) => elm.is_done().cloned(),
            OverlayEnum::ColSelector(elm) => elm.is_done().cloned(),
            OverlayEnum::Uploader(elm) => elm.is_done().cloned(),
        }
    }
}

impl Debug for OverlayEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Review(_) => f.debug_tuple("Review").finish(),
            Self::Colviewer(_) => f.debug_tuple("Colviewer").finish(),
            Self::Text(_) => f.debug_tuple("Text").finish(),
            Self::CardViewer(_) => f.debug_tuple("card viewer").finish(),
            Self::CardSelector(_) => f.debug_tuple("card selector").finish(),
            Self::YesNo(_) => f.debug_tuple("yesno").finish(),
            Self::ColSelector(_) => f.debug_tuple("col selector").finish(),
            Self::Uploader(_) => f.debug_tuple("uploader").finish(),
        }
    }
}

#[component]
pub fn Overender(overlay: Signal<Option<OverlayEnum>>, root: Element) -> Element {
    let is_done = overlay.as_ref().is_some_and(|ol| ol.is_done());

    if is_done {
        overlay.set(None);
    }

    rsx! {
        match overlay.cloned() {
            Some(elm) => rsx!{
                div {
                    button {
                        onclick: move |_| {
                            overlay.clone().set(None);
                        },
                        "❌"
                    }

                    match elm {
                        OverlayEnum::Review(elm) => {
                            rsx!{
                                ReviewRender {
                                    front: elm.front.cloned().unwrap_or_default(),
                                    back: elm.back.cloned().unwrap_or_default(),
                                    card: elm.card.cloned().unwrap().unwrap(),
                                    queue: elm.queue.clone(),
                                    show_backside: elm.show_backside.clone(),
                                    tot: elm.tot_len,
                                    overlay: elm.overlay.clone(),
                                    dependencies:elm.dependencies.clone(),
                                }
                            }
                        },
                        OverlayEnum::Colviewer(elm) => elm.render(),
                        OverlayEnum::Text(elm) => elm.render(),
                        OverlayEnum::CardViewer(elm) => elm.render(),
                        OverlayEnum::YesNo(elm) => elm.render(),
                        OverlayEnum::ColSelector(elm) => elm.render(),
                        OverlayEnum::CardSelector(elm) => elm.render(),
                        OverlayEnum::Uploader(elm) => elm.render(),
                    }
                }
            },
            None => root ,
        }
    }
}

#[derive(Clone)]
pub struct ReviewPage {
    filter: FilterEditor,
    cardfilter: Memo<CardFilter>,
    collections: Signal<Vec<(Collection, RecallDist)>>,
    overlay: Signal<Option<OverlayEnum>>,
}

impl ReviewPage {
    pub fn new() -> Self {
        let filter = FilterEditor::new_default();
        let cardfilter = filter.memo();
        let selv = Self {
            filter,
            cardfilter,
            collections: Default::default(),
            overlay: Default::default(),
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

    let overlay = state.overlay.clone();

    rsx! {
        Overender {
            overlay,
            root: rsx!{
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

    let overlay = state.overlay.clone();

    rsx! {
        div {
            class: "flex flex-col max-w-[550px] mr-5",

            button {
                class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-8",
                onclick: move |_| {
                    let filter = filter.clone();
                    spawn(async move {
                        let cards = APP.read().load_all(Some(filter)).await;
                        let revses = OverlayEnum::Review(ReviewState::new(cards).await);
                        overlay.clone().set(Some(revses));
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
                                    let viewer = OverlayEnum::Colviewer(ColViewer::new(col.id).await);
                                    overlay.clone().set(Some(viewer));
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
                                    let session = OverlayEnum::Review(ReviewState::new_with_filter(cards, filter).await);
                                    overlay.clone().set(Some(session));
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

                    let txt = OverlayEnum::Text(TextInput::new("add collection".to_string(), Arc::new(Box::new(f))));
                    overlay.clone().set(Some(txt));
                },
                "add collection"
            }
        }
    }
}
