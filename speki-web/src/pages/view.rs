use super::*;

#[component]
pub fn View(id: String) -> Element {
    let uuid: Uuid = id.parse().unwrap();

    let front = use_signal(|| "".to_string());

    spawn(async move {
        let card = IndexBaseProvider::new("/foobar")
            .load_card(CardId(uuid))
            .await
            .unwrap();
        let card = format!("{card:?}");
        front.clone().set(card);
    });

    rsx! {
        "{front}"
    }
}
