use speki_dto::BackSide;

use super::*;

pub enum CardCharacteristic {
    Any,
    Class,
    Instance,
    SubclassOf(CardId),
}

impl CardCharacteristic {
    pub fn card_matches(&self, card: CardId) -> bool {
        let card = Card::from_id(card).unwrap();

        match self {
            CardCharacteristic::Any => true,
            CardCharacteristic::Class => card.is_class(),
            CardCharacteristic::Instance => card.is_instance(),
            CardCharacteristic::SubclassOf(card_id) => {
                card.load_ancestor_classes().contains(&card_id)
            }
        }
    }
}

pub enum BackConstraint {
    Time,
    Card(CardCharacteristic),
    List(Vec<CardCharacteristic>),
}

pub fn display_backside(backside: &BackSide) -> String {
    match backside {
        BackSide::Trivial => format!("…"),
        BackSide::Time(time) => format!("🕒 {}", time),
        BackSide::Text(s) => s.to_owned(),
        BackSide::Card(id) => format!("→ {}", Card::from_id(*id).unwrap().print()),
        BackSide::List(list) => format!(
            "→ [{}]",
            list.iter()
                .map(|id| Card::from_id(*id).unwrap().print())
                .collect::<Vec<String>>()
                .join(", ")
        ),
    }
}
