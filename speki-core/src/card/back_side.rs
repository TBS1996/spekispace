use speki_dto::BackSide;

use super::*;

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
