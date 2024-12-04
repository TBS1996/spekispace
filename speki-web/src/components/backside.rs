/*
pub enum BackPut {
    Text(Signal<String>),
    Ref(Signal<CardId>),
}

fn render_ref(sig: Signal<CardId>) -> Element {
    let inp = use_signal(String::new);
    let id = sig.cloned();
    spawn(async move {
        let mut input = inp.clone();
        //let app = use_context::<App>();
        //let card = app.0.load_card(id).await.unwrap();
        //let back = card.print().await;
        //input.set(back);
    });

    rsx! {
        input {
            class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
            value: "{inp}",
            disabled: "true",
        }
    }
}
fn render_text(mut sig: Signal<String>) -> Element {
    rsx! {
        input {
            class: "w-full border border-gray-300 rounded-md p-2 mb-4 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
            value: "{sig}",
            oninput: move |evt| sig.set(evt.value()),
        }
    }
}

impl BackPut {
    fn view(&self) -> Element {
        rsx! {
            match self {
                Self::Ref(sig) => render_ref(sig.clone()),
                Self::Text(sig) => render_text(sig.clone()),
            }
        }
    }
}

*/
