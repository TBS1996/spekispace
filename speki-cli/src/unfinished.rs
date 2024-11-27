use crate::utils::{clear_terminal, notify};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use rand::seq::SliceRandom;
use speki_core::{App, NormalCard};

pub async fn unfinished(app: &App) {
    let filter = "finished == false & suspended == false".to_string();
    let mut cards = app.cards_filtered(filter).await;
    if cards.is_empty() {
        clear_terminal();
        notify("no unfinished cards");
        return;
    }

    cards.shuffle(&mut rand::thread_rng());

    for card_id in cards {
        loop {
            let front = app.load_card(card_id).await.unwrap().print().await;
            clear_terminal();

            let input: String = Input::new()
                .with_prompt(front.clone())
                .allow_empty(true)
                .interact_text()
                .expect("Failed to read input");

            if input.is_empty() {
                break;
            }

            let options = vec!["confirm", "keep editing", "next card", "exit"];
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("save answer and mark card as finished?")
                .items(&options)
                .default(0)
                .interact()
                .expect("Failed to make selection");

            match selection {
                0 => {
                    app.load_card(card_id)
                        .await
                        .unwrap()
                        .into_type(NormalCard {
                            front,
                            back: input.into(),
                        })
                        .await;
                    break;
                }
                1 => continue,
                2 => break,
                3 => return,
                _ => panic!(),
            }
        }
    }
}
