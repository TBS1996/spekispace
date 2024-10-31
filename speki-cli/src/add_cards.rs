use crate::{
    create_card,
    incread::{inc_path, textstuff},
    utils::{choose_folder, clear_terminal, get_input_opt, notify},
};
use dialoguer::{theme::ColorfulTheme, Select};
use speki_core::{categories::Category, common::filename_sanitizer, CType};
use std::{fs::read_to_string, io::Write, path::PathBuf, str::FromStr};

pub fn add_cards() {
    let category = choose_folder(None);

    loop {
        clear_terminal();
        if create_card(CType::Normal, &category).is_none() {
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
        "Import",
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
        4 => import(),
        5 => return,
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

fn import() {
    notify("pick a csv file where the left side is the question and the right side the answer");

    if let Some(path) = get_input_opt("file path") {
        let path = match PathBuf::from_str(&path) {
            Ok(path) => path,
            Err(e) => {
                notify(&format!("failed to parse input as a valid path: {:?}", e));
                return;
            }
        };

        if !path.exists() {
            notify("provided path does not point to a file");
            return;
        }

        let import: String = read_to_string(&path).unwrap();
        let filename = path.file_stem().unwrap().to_str().unwrap();
        let filename = filename_sanitizer(filename);
        let category = Category::default().join("imports").join(&filename);

        let mut cards = vec![];

        for line in import.lines() {
            let (front, back) = line.split_once(";").unwrap();
            let card = speki_core::add_card(front.to_string(), back.to_string(), &category);
            cards.push(card);
        }

        let card_qty = cards.len();

        notify(&format!(
            "imported {} cards to following path: {:#?}",
            card_qty,
            category.as_path()
        ));
    }
}
