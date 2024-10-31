use crate::{
    create_card,
    incread::{inc_path, textstuff},
    utils::{clear_terminal, get_input_opt, notify},
};
use dialoguer::{theme::ColorfulTheme, Select};
use speki_core::CType;
use std::io::Write;

pub fn add_cards() {
    loop {
        clear_terminal();
        if create_card(CType::Normal).is_none() {
            break;
        }
    }
}

pub async fn add_cards_menu() {
    let items = vec![
        "New cards",
        "Unfinished cards",
        "Add wikipedia article",
        "Incremental reading",
        "exit",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&items)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => crate::add_cards::add_cards(),
        1 => crate::unfinished::unfinished(),
        2 => add_wikipedia(),
        3 => textstuff(),
        4 => return,
        _ => panic!(),
    }
}

fn add_wikipedia() {
    use std::fs::File;
    use std::thread;
    use wikipedia::http::default::Client;
    use wikipedia::Wikipedia;

    let Some(input) = get_input_opt("wikipedia article") else {
        return;
    };

    let handle = thread::spawn(move || {
        let wiki = Wikipedia::<Client>::default();
        let page = wiki.page_from_title(input);
        let content = match page.get_content() {
            Ok(content) => content,
            Err(_) => return Err("unable to fetch wikipedia article"),
        };
        let title = match page.get_title() {
            Ok(title) => title,
            Err(_) => return Err("unable to fetch wikipedia article"),
        };
        Ok((title, content))
    });

    match handle.join() {
        Ok(Ok((title, content))) => {
            let path = inc_path().join(&title);
            let mut file = File::create(&path).unwrap();
            file.write_all(content.as_bytes()).unwrap();
            notify(format!("imported following title: {}", title));
        }
        Ok(Err(msg)) => {
            notify(msg);
        }
        Err(_) => {
            notify("failed to join thread");
        }
    }
}
