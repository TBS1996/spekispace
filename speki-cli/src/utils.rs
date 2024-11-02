use dialoguer::{theme::ColorfulTheme, Input, Select};
use speki_core::{AnyType, App, Attribute, Card, CardId};
use speki_dto::AttributeId;
use speki_fs::paths;
use std::path::Path;
use std::process::Command;

#[allow(dead_code)]
pub fn notify(msg: impl Into<String>) {
    clear_terminal();
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(msg.into())
        .items(&["continue"])
        .default(0)
        .interact()
        .unwrap();
}

pub fn select_from_subclass_cards(app: &App, class: CardId) -> Option<CardId> {
    let cards: Vec<Card<AnyType>> = app
        .foobar
        .load_all_cards()
        .into_iter()
        .filter(|card| card.load_ancestor_classes().contains(&class))
        .collect();

    enumselector::select_item_with_formatter(cards, |card: &Card<AnyType>| card.print())?
        .id()
        .into()
}

pub fn select_from_all_instance_cards(app: &App) -> Option<CardId> {
    let cards: Vec<Card<AnyType>> = app
        .foobar
        .load_all_cards()
        .into_iter()
        .filter(|card| card.is_instance())
        .collect();
    enumselector::select_item_with_formatter(cards, |card: &Card<AnyType>| card.print())?
        .id()
        .into()
}

pub fn select_from_all_class_cards(app: &App) -> Option<CardId> {
    let cards = app.foobar.load_all_cards();
    enumselector::select_item_with_formatter(cards, |card: &Card<AnyType>| card.print())?
        .id()
        .into()
}

pub fn select_from_class_attributes(app: &App, class: CardId) -> Option<AttributeId> {
    enumselector::select_item_with_formatter(
        Attribute::load_from_class_only(app, class),
        |attr: &Attribute| attr.pattern().to_owned(),
    )?
    .id
    .into()
}

pub fn select_from_attributes(attributes: Vec<Attribute>) -> Option<AttributeId> {
    enumselector::select_item_with_formatter(attributes, |attr: &Attribute| {
        attr.pattern().to_owned()
    })?
    .id
    .into()
}

pub fn select_from_all_cards(app: &App) -> Option<CardId> {
    enumselector::select_item_with_formatter(
        app.foobar.load_all_cards(),
        |card: &Card<AnyType>| card.print().to_owned(),
    )?
    .id()
    .into()
}

pub fn clear_terminal() {
    use std::io::Write;
    print!("\x1B[2J\x1B[H");
    std::io::stdout().flush().unwrap();
}

pub fn get_lines(text: &str, line_width: usize, height: usize, position: usize) -> Vec<String> {
    let mut output = vec![];
    let lines = cli_justify::justify(text, line_width);

    let mut sum = 0;
    for line in lines {
        sum += line.chars().count();
        if sum >= position {
            output.push(line);
        }

        if output.len() >= height {
            return output;
        }
    }

    output
}

pub fn get_input_opt(prompt: &str) -> Option<String> {
    let s: String = Input::new()
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()
        .expect("Failed to read input");

    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn get_input(prompt: &str) -> String {
    get_input_opt(prompt).unwrap_or_default()
}

/*

diff reasons we can't sync:

1. not signed in
2. no repo
3. repo, but remote not set
4. remote set, but no access



*/

mod cli_justify {

    fn split_at_char(s: &str, n: usize) -> (&str, Option<&str>) {
        for (char_index, (i, _)) in s.char_indices().enumerate() {
            if char_index == n {
                let (w1, w2) = s.split_at(i);
                return (w1, Some(w2));
            }
        }

        (s, None)
    }

    pub fn justify(text: &str, line_width: usize) -> Vec<String> {
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        let mut lines: Vec<String> = Vec::new();

        for paragraph in paragraphs {
            let raw_words: Vec<&str> = paragraph.split_whitespace().collect();
            let mut words = vec![];

            for mut word in raw_words {
                while let (w1, Some(w2)) = split_at_char(word, line_width) {
                    words.push(w1);
                    word = w2;
                }

                words.push(word);
            }

            let mut line: Vec<&str> = Vec::new();
            let mut len = 0;

            for word in words {
                if len + word.len() > line_width {
                    lines.push(justify_line(&line, line_width));
                    line.clear();
                    len = 0;
                }
                line.push(word);
                len += word.len() + 1; // +1 for space
            }

            // Add the last line of the paragraph
            if !line.is_empty() {
                lines.push(line.join(" "));
            }

            // Add a blank line after each paragraph to preserve paragraph breaks
            lines.push(String::new());
        }

        lines
    }

    fn justify_line(line: &[&str], line_width: usize) -> String {
        let word_len: usize = line.iter().map(|s| s.len()).sum();
        let spaces = line_width as i64 - word_len as i64;
        let spaces = spaces.max(0) as usize;

        let line_len_div = if line.len() > 1 { line.len() - 1 } else { 1 };

        let each_space = spaces / line_len_div;
        let extra_space = spaces % line_len_div;

        let mut justified = String::new();
        for (i, word) in line.iter().enumerate() {
            justified.push_str(word);
            if i < line.len() - 1 {
                let mut space = " ".repeat(each_space);
                if i < extra_space {
                    space.push(' ');
                }
                justified.push_str(&space);
            }
        }

        justified
    }
}

pub fn edit_with_vim(id: CardId) {
    let p = paths::get_cards_path().join(id.to_string());
    open_file_with_vim(&p);
}

pub fn open_file_with_vim(path: &Path) {
    Command::new("nvim").arg(path).status().unwrap();
}
