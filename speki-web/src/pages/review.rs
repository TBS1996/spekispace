use std::{fmt::Debug, sync::Arc};

use dioxus::prelude::*;
use speki_core::{
    cardfilter::CardFilter,
    collection::{Collection, DynCard},
    ledger::{CollectionAction, CollectionEvent},
};
use tracing::info;
use uuid::Uuid;

use crate::{
    components::{FilterComp, FilterEditor},
    overlays::{
        colviewer::CollectionEditor,
        reviewsession::{ReviewSession, ReviewState},
        textinput::TextInput,
        Overender, OverlayEnum,
    },
    APP,
};

/*

idea:
allow a card to have to be typed out

also, free text should be able to embed ids directly into it

like a text can be instead of 'it uses https', itll be 'it uses [382..]' where that
refer to the id of https.


priority idea:

to figure out which cards have higher/lower priority:

show the user two random cards, ask which is higher priority (or about same)
figure out automatically what makes certain cards more important than others based on their dependency graph

like some kinda smart algorithm that figures out commonalities by why certain cards more important than other cards.
based on this, it can estimtae which cards more important than others based on which dependencies/dependents they have


hmm i guess if A depends on B, and B depends on C, then by definition, C is _at least_ more important than A though.
that could be another constraint.

in a way you can say if A and B depends on C then C's priority is at least the sum of A and B? this would imply some kinda absolute score of priority rather
than just a priority queue like in supermemo.

idk what this value would represent though. maybe like the utility of knowing it, like, how much would your life improve by you knowing this. or maybe how much you'd pay to know it.

i guess i dont have to expose the value or anything.

maybe just give by default a value of 1.0 to all cards. then it'll based on the a bunch of constraints just calculate all the values so theyre consistent with each other.

like on the "A and B depends on C" thing then it could be something like value of A is 0.5, value of B is 0.5, value of C is 1.0. or 1,1,2. doesn't matter.
speki would just make a bunch of relative constraints and then figure out the score in a way that is consistent but the value doesn't represent anything by itself.

so, constraints:

1. a card's value must be equal or higher than the sum of all its transitive dependents.
2. if user rates A to be more valuable than B, then A's value must be higher than B.
3. make a best-effort value estimate based on patterns from user rating of cards.

for example, since i work at cognite all the cards that have 'cognite' as a dependency would be rated very highly by me on the toss-ups.
it'll figure out that cards with cognite as a dependency are pretty important.
so when i add a new card and put cognite as a dependency, it'll automatically be ranked highly because of this.

if i later on a toss-up rank it lower than some non-cognite thing then ofc that would be respected.

so the algorithm, i guess on first pass, use the "weights" based on dependencies to give each card a value.
then, maybe take all the toss-ups to rank the specific cards ranked higher or lower than each other
then maybe ensure the constraint about value must be higher than sum of its transitive dependents

but how to handle conflicts?

if user ranks A higher than B, B higher than C, and C higher than A? in that case i think must ask for clarification somehow and based on that delete some invalid comparison.
maybe then ask for 3 at a time? like rank A, B, and C. can i resolve a chain larger than 3 without asking for more than 3 at a time? this might not represent an invalid
choice by the user if some time have passed and the priorities simply have changed.


conflict can also occur if user ranks A more important than B, even though A depends on B. in that case it's a user error, since by definition it's more important to know the dependencies first.


ok waitt so lets see

glossary: high-level items, stuff that many things depends on
low-level, like the leaf stuff that have few dependents.

the pair-wise comparisons will let the user rate mainly low-level stuff
from this, we can figure out certain patterns like that things with for example cognite as dependency are rated more high in general
when you add a new item with cognite as dependency the "first guess" is that it has higher priority

that way you kinda have a two-way flow of value stream. user rates low-level things, we figure out which high-level stuff is more important, then new low-level things get priority based on that.

so the algo, yeah, it'll do a first-pass as i mentioned earlier to assign just based on that, then add those pairwise and dependency constraints.

im thinking the value of an object can still be valued higher than the sum of its dependents though, it can happen since the user can rank an item higher than another item even though its dependents arent that important.
this would represent the utility of the knowledge beyond the dependents it enables, like how important it is by itself to know it, and/or dependents that the user have not added as cards (yet).

however, it's a hard constraint that it can't be worth _less_ than its dependents.

ok the algo should try to put every ranked item in a list ordered by the rankings, where the constraint is, no cycles like A > B > C > A. and the dependency thing like A > B where A depends on B.
this list wouldn't store the values at all, it's merely ordered.

the step after this would be to assign actual values.

the first step here to add that ML inference based on patterns of their dependencies, like for all cards not just the ones in that list?
then some normalization stuff like ensuring the constraints wrt sum of dependents and the pairwise thing

actually im a bit unsure how to proceed here.


*/

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

            for col in _cols.clone() {
                out.push((col, RecallDist::default()));
            }
            cols.clone().set(out);

            let mut out = vec![];

            let mut futs = vec![];

            for col in _cols {
                futs.push(async move {
                    let dist = RecallDist::new(col.clone()).await;
                    (col, dist)
                });
            }

            for (col, dist) in futures::future::join_all(futs).await {
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

    let overlay = state.overlay.clone();

    rsx! {
        Overender {
            overlay,
            root: rsx!{
                div {
                    class: "flex flex-row items-start min-h-screen space-y-4 justify-start w-full",
                    FilterComp {editor}
                    RenderCols{
                        filter: state.filter.to_filter(),
                        collections: state.collections.clone(),
                        overlay: state.overlay.clone(),
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
        //return selv;

        for card in col.expand(APP.read().inner().card_provider()).await {
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
       //     class: "flex flex-col max-w-[550px] mr-5",

            div {
                button {
                    class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-8",
                    onclick: move |_| {
                        let filter = filter.clone();
                        spawn(async move {
                            let session = ReviewSession::new(vec![DynCard::Any], filter).await;
                            let revses = OverlayEnum::Review(ReviewState::new(session));
                            overlay.clone().set(Some(revses));
                        });
                    },
                    "review all"
                    }

                button {
                    class: "inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                    onclick: move |_| {
                        let done = Signal::new_in_scope(false, ScopeId::APP);
                        let f = move |name: String| {
                            info!("new collection made!");
                            spawn(async move {
                                info!("saving it!");
                                let event = CollectionEvent::new(Uuid::new_v4(), CollectionAction::SetName(name));
                                APP.read().inner().provider.run_event(event);
                                done.clone().set(true);
                                info!("saved it!");
                            });
                            info!("bye");
                        };

                        let txt = OverlayEnum::Text(TextInput::new("add collection".to_string(), Arc::new(Box::new(f)), done));
                        overlay.clone().set(Some(txt));
                    },
                    "add collection"
                }
            }

            for (col, dist, filter) in colfil {
                div {
                    class: "flex flex-col mb-4",
                    div {
                    class: "flex flex-row",
                        button {
                            class: "inline-flex items-center text-white bg-gray-800 border-0 py-1 px-3 focus:outline-none hover:bg-gray-700 rounded text-base mb-2",
                            onclick: move |_| {
                                let filter = filter.clone();
                                spawn(async move {
                                    let col = APP.read().load_collection(col.id).await;
                                    let provider = APP.read().inner().card_provider.clone();
                                    let mut cards = vec![];
                                    for card in col.dyncards {
                                        cards.extend(card.expand(provider.clone(), Default::default()).await);
                                    }
                                    let session = OverlayEnum::Review(ReviewState::new_with_filter(cards, filter).await);
                                    overlay.clone().set(Some(session));
                                });
                            },
                            "{col.name}"
                        }
                        button {
                            class: "ml-auto inline-flex items-center text-white bg-blue-700 border-0 py-1 px-3 focus:outline-none hover:bg-blue-900 rounded text-base mb-5",
                            onclick: move |_|{
                                spawn(async move {
                                    let viewer = OverlayEnum::Colviewer(CollectionEditor::new(col.id).await);
                                    overlay.clone().set(Some(viewer));
                                });
                            },
                            "edit"
                        }
                    }

                    RecallBar { dist  }
                }
            }

        }
    }
}
