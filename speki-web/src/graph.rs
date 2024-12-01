use std::{collections::BTreeSet, sync::Arc};

use dioxus::hooks::use_context;
use speki_core::{AnyType, Card};
use tracing::info;

use crate::App;

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub color: String,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

/// Loads all the recursive dependents and dependencies of a given card.
pub async fn connected_nodes_and_edges(
    card: Arc<Card<AnyType>>,
) -> (BTreeSet<Edge>, BTreeSet<Node>) {
    let app = use_context::<App>();
    let mut all_cards = BTreeSet::default();
    let mut edges = BTreeSet::default();
    let mut nodes = BTreeSet::default();

    for card in card.all_dependencies().await {
        let card = app.as_ref().load_card(card).await.unwrap();
        all_cards.insert(card);
    }

    for card in card.all_dependents().await {
        let card = app.as_ref().load_card(card).await.unwrap();
        all_cards.insert(card);
    }

    for card in &all_cards {
        let node = Node {
            id: card.id.into_inner().to_string(),
            label: card.print().await,
            color: "#32a852".to_string(),
        };

        nodes.insert(node);
    }

    let node = Node {
        id: card.id.into_inner().to_string(),
        label: card.print().await,
        color: "#34ebdb".to_string(),
    };

    nodes.insert(node);
    all_cards.insert((*card).clone());

    for card in &all_cards {
        let from = card.id.into_inner().to_string();

        for dep in card.dependency_ids().await {
            let to = dep.into_inner().to_string();

            let edge = Edge {
                from: from.clone(),
                to: to.clone(),
            };

            if nodes.iter().find(|node| &node.id == &to).is_some() {
                edges.insert(edge);
            }
        }
    }

    info!("nodes:");
    for node in &nodes {
        info!("node: {}", node.id);
    }

    info!("edges:");
    for edge in &edges {
        info!("from: {}; to: {}", edge.from, edge.to);
    }

    (edges, nodes)
}
