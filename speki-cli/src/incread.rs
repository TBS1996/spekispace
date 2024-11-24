use cli_epub_to_text::epub_to_text;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use serde::{Deserialize, Serialize};
use speki_core::current_time;
use speki_core::App;
use speki_core::CardId;
use speki_fs::paths;
use std::collections::HashMap;
use std::fs::{self, read_to_string};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::add_any_card;
use crate::review::view_card;
use crate::utils::get_lines;
use crate::utils::{clear_terminal, notify};

pub fn inc_path() -> PathBuf {
    let path = paths::get_share_path().join("texts");
    fs::create_dir_all(&path).unwrap();
    path
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TextFile {
    path: PathBuf,
    position: usize,
    length: usize,
    #[serde(default = "current_time")]
    time_added: Duration,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    added_cards: Vec<(CardId, usize)>,
    category: Option<PathBuf>,
}

impl TextFile {
    pub async fn min_rec_recall_rate(&self, app: &App) -> Option<f32> {
        if self.added_cards.is_empty() {
            return Some(1.0);
        }

        let mut recall_rate: f32 = 1.0;
        for (card, _) in &self.added_cards {
            let card = app.load_card(*card).await.unwrap();
            recall_rate = recall_rate.min(card.min_rec_recall_rate().await?);
        }

        Some(recall_rate)
    }

    pub fn name(&self) -> &str {
        self.path.file_stem().unwrap().to_str().unwrap()
    }

    pub fn _avg_daily_progress(&self) -> usize {
        let days_passed = (current_time() - self.time_added).as_secs_f32();
        let avg = self.position() as f32 / days_passed;
        avg as usize
    }

    pub fn progress_percentage(&self) -> f32 {
        (self.position as f32 / self.length as f32) * 100.
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn save_pos(&mut self, new_pos: usize) {
        let mut txt = TextProgress::xload();
        self.position = new_pos;
        txt.0.insert(self.path.clone(), self.clone());
        txt.save();
    }

    pub fn position_decrement(&mut self, dec: usize) {
        let new_pos = if self.position < dec {
            0
        } else {
            self.position - dec
        };

        self.save_pos(new_pos);
    }

    pub fn position_increment(&mut self, inc: usize) {
        let new_pos = self.length.min(self.position + inc);
        self.save_pos(new_pos);
    }

    pub fn load_all() -> Vec<Self> {
        TextProgress::xload()
            .0
            .into_iter()
            .map(|(_, val)| val)
            .collect()
    }

    pub fn add_card(&mut self, card: CardId) {
        let mut txt = TextProgress::xload();
        self.added_cards.push((card, self.position));
        txt.0.insert(self.path.clone(), self.clone());
        txt.save();
    }

    pub fn load_text(&self) -> String {
        text_load(&self.path).unwrap()
    }
}

fn text_load(p: &Path) -> Option<String> {
    if p.extension().is_some_and(|ext| ext == "pdf") {
        cli_pdf_to_text::pdf_to_text(p.as_os_str().to_str().unwrap()).ok()
    } else if p.extension().is_some_and(|ext| ext == "epub") {
        epub_to_text(p.as_os_str().to_str().unwrap()).ok()
    } else {
        read_to_string(&p).ok()
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct TextProgress(HashMap<PathBuf, TextFile>);

impl TextProgress {
    fn save(&self) {
        let s: String = serde_json::to_string_pretty(&self).unwrap();
        let path = Self::path();
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(s.as_bytes()).unwrap();
    }

    fn path() -> PathBuf {
        paths::get_share_path().join("bookmarks")
    }

    fn load() -> Option<Self> {
        let s: String = read_to_string(&Self::path()).ok()?;
        serde_json::from_str(&s).ok()
    }

    fn xload() -> Self {
        let mut txts = Self::load().unwrap_or_default();
        let files = get_text_files(&inc_path()).unwrap();

        for file in files {
            if !txts.0.contains_key(&file) {
                let length = match text_load(&file) {
                    Some(txt) => txt.chars().count(),
                    None => continue,
                };

                txts.0.insert(
                    file.clone(),
                    TextFile {
                        length,
                        path: file,
                        position: 0,
                        time_added: current_time(),
                        added_cards: Default::default(),
                        category: None,
                    },
                );
            }
        }

        txts
    }
}

async fn select_text(textfiles: Vec<TextFile>, app: &App) -> TextFile {
    let mut named = vec![];

    for p in &textfiles {
        let s = format!(
            "{:.1}%, {:.1}%: {}",
            p.progress_percentage(),
            p.min_rec_recall_rate(app).await.unwrap_or_default() * 100.,
            p.name()
        );
        named.push(s);
    }

    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("")
        .items(&named)
        .default(0)
        .interact()
        .expect("Failed to make selection");

    textfiles[idx].clone()
}

pub async fn textstuff(app: &App) {
    clear_terminal();
    //    let paths = get_text_files(&inc_path()).unwrap();
    let textfiles = TextFile::load_all();
    if textfiles.is_empty() {
        notify(format!("no available texts. click 'inspect texts' in main menu and add textfiles to get started"));
        return;
    }

    let mut textfile = select_text(textfiles, app).await;
    let text = textfile.load_text();

    let opts = [
        "add card",
        "go forward",
        "go back",
        "view last card",
        "exit",
    ];
    let mut menu_position = 0;

    let menu_size = opts.len() as u16 + 7;

    loop {
        clear_terminal();
        let (height, width) = console::Term::stdout().size();
        let free_space = if height > menu_size {
            height - menu_size
        } else {
            0
        };

        let line_qty = 20.min(free_space);

        let s = get_lines(
            &text,
            50.min(width as usize),
            line_qty as usize,
            textfile.position(),
        );

        let char_len = s.clone().join("").chars().count();

        for line in s {
            println!("{}", line);
        }

        let idx = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("")
            .items(&opts)
            .default(menu_position)
            .interact()
            .expect("Failed to make selection");

        menu_position = idx;
        match idx {
            0 => {
                if let Some(id) = add_any_card(app).await {
                    textfile.add_card(id);
                };
            }
            1 => textfile.position_increment(char_len),
            2 => textfile.position_decrement(char_len),
            3 => {
                if let Some((card, _)) = textfile.added_cards.last() {
                    view_card(app, *card, false).await;
                }
            }
            4 => return,
            _ => panic!(),
        }
    }
}

fn get_text_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut text_files = Vec::new();

    if dir.is_dir() {
        let x = fs::read_dir(dir)?;

        for entry in x {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                text_files.push(path);
            }
        }
    }

    Ok(text_files)
}
