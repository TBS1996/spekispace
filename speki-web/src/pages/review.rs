use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use speki_core::{cardfilter::CardFilter, collection::Collection};

use crate::{
    components::{FilterComp, FilterEditor},
    overlays::{
        card_selector::{CardSelector, CardSelectorRender},
        cardviewer::{CardViewer, CardViewerRender},
        colviewer::{ColViewRender, ColViewer},
        itemselector::{ItemSelector, ItemSelectorRender},
        reviewsession::{ReviewRender, ReviewState},
        textinput::{TextInput, TextInputRender},
        uploader::{UploadRender, Uploader},
        yesno::{Yesno, YesnoRender},
    },
    APP, IS_SHORT,
};

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
            OverlayEnum::Colviewer(elm) => elm.done.cloned(),
            OverlayEnum::Text(elm) => elm.done.cloned(),
            OverlayEnum::CardViewer(elm) => elm.is_done.cloned(),
            OverlayEnum::CardSelector(elm) => elm.done.cloned(),
            OverlayEnum::YesNo(elm) => elm.done.cloned(),
            OverlayEnum::ColSelector(elm) => elm.done.cloned(),
            OverlayEnum::Uploader(elm) => elm.done.cloned(),
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
            None => root,
            Some(elm) => rsx!{
                div {
                    button {
                        onclick: move |_| {
                            overlay.clone().set(None);
                        },
                        "❌"
                    }

                    match elm {
                        OverlayEnum::Review(elm) =>
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
                            },
                        OverlayEnum::Colviewer(elm) => rsx!{
                            ColViewRender{
                                col: elm.col.clone(),
                                colname:  elm.colname.clone(),
                                done:  elm.done.clone(),
                                entries: elm.entries.clone(),
                                cardselector: elm.cardselector.clone(),
                                colselector: elm.colselector.clone(),
                                instance_selector: elm.instance_selector.clone(),
                                dependents_selector: elm.dependents_selector.clone(),
                                dynty: elm.dynty.clone(),
                            }
                        },
                        OverlayEnum::Text(elm) => rsx!{
                            TextInputRender {

    question: elm.question.clone(),
    input_value: elm.input_value.clone(),
    done: elm.done.clone(),
    on_submit: elm.on_submit.clone(),



                            }
                        },
                        OverlayEnum::CardViewer(elm) => rsx!{


        CardViewerRender {
            title: elm.title.clone(),
            front: elm.front.clone(),
            back: elm.back.clone(),
            concept: elm.concept.clone(),
            dependencies: elm.dependencies.clone(),
            dependents: elm.dependents.clone(),
            graph: elm.graph.clone(),
            save_hook: elm.save_hook.clone(),
            is_done: elm.is_done.clone(),
            old_card: elm.old_card.clone(),
            old_meta: elm.old_meta.clone(),
            filter: elm.filter.clone(),
            tempnode: elm.tempnode.clone(),
            allowed_cards: elm.allowed_cards.clone(),
            overlay: elm.overlay.clone(),
        }



                        },
                        OverlayEnum::YesNo(elm) => rsx! {
                            YesnoRender {
                                question: elm.question.clone(),
                                done: elm.done.clone(),
                                on_yes: elm.on_yes.clone(),
                            }
                        },
                        OverlayEnum::ColSelector(elm) => rsx!{
                            ItemSelectorRender {
                                items: elm.items.clone(),
                                on_selected: elm.on_selected.clone(),
                                done: elm.done.clone(),
                            }
                        },
                        OverlayEnum::CardSelector(elm) => rsx!{
                            CardSelectorRender {
                            title: elm.title.clone(),
                            search: elm.search.clone(),
                            on_card_selected: elm.on_card_selected.clone(),
                            all_cards: elm.all_cards.clone(),
                            filtered_cards: elm.filtered_cards.clone(),
                            allow_new: elm.allow_new.clone(),
                            done: elm.done.clone(),
                            filter: elm.filter.clone(),
                            dependents: elm.dependents.clone(),
                            allowed_cards: elm.allowed_cards.clone(),
                            filtereditor: elm.filtereditor.clone(),
                            filtermemo: elm.filtermemo.clone(),
                            overlay: elm.overlay.clone(),
                        }
                    },
                        OverlayEnum::Uploader(elm) => rsx!{


        UploadRender {
            content: elm.content.clone(),
            regex: elm.regex.clone(),
            cards: elm.cards.clone(),
            dropdown: elm.dropdown.clone(),
            done: elm.done.clone(),
            concept: elm.concept.clone(),
            overlay: elm.overlay.clone(),
        }



                        },
                    }
                }
            },
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
                        RenderCols{
                            filter: state.filter.to_filter(),
                            collections: state.collections.clone(),
                            overlay: state.overlay.clone(),
                        }
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

#[component]
fn RenderCols(
    filter: CardFilter,
    collections: Signal<Vec<(Collection, RecallDist)>>,
    overlay: Signal<Option<OverlayEnum>>,
) -> Element {
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
                        let revses = OverlayEnum::Review(ReviewState::new(cards));
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
