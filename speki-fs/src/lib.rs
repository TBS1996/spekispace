use rayon::prelude::*;
use speki_dto::Config;
use speki_dto::{AttributeDTO, AttributeId, CardId, RawCard, Recall, Review, SpekiProvider};
use std::{
    fs::{self, read_to_string},
    io::{Read, Write},
    path::Path,
    str::FromStr,
    time::Duration,
};

pub mod paths;

fn load_files<P: AsRef<Path>>(folder_path: P) -> std::io::Result<Vec<String>> {
    let entries = fs::read_dir(folder_path)?.collect::<Result<Vec<_>, std::io::Error>>()?;

    let contents: Vec<String> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() {
                let mut file_content = String::new();
                if let Ok(mut file) = fs::File::open(&path) {
                    if file.read_to_string(&mut file_content).is_ok() {
                        return Some(file_content);
                    }
                }
            }
            None
        })
        .collect();

    Ok(contents)
}

pub struct FileProvider;

use async_trait::async_trait;

#[async_trait(?Send)]
impl SpekiProvider for FileProvider {
    async fn load_all_cards(&self) -> Vec<RawCard> {
        load_files(paths::get_cards_path())
            .unwrap()
            .into_par_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn last_modified_card(&self, id: CardId) -> Duration {
        todo!()
    }

    async fn last_modified_reviews(&self, id: CardId) -> Option<Duration> {
        todo!()
    }

    async fn save_card(&self, card: RawCard) {
        let s: String = toml::to_string(&card).unwrap();
        let path = paths::get_cards_path().join(card.id.to_string());
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
    }

    async fn load_card(&self, id: CardId) -> Option<RawCard> {
        let path = paths::get_cards_path().join(id.to_string());
        if !path.exists() {
            None
        } else {
            let s = fs::read_to_string(&path).unwrap();
            toml::from_str(&s).unwrap()
        }
    }

    async fn load_all_attributes(&self) -> Vec<AttributeDTO> {
        load_files(paths::get_attributes_path())
            .unwrap()
            .into_par_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    async fn save_attribute(&self, attribute: AttributeDTO) {
        let s: String = toml::to_string(&attribute).unwrap();
        let path = paths::get_attributes_path().join(attribute.id.into_inner().to_string());
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
    }

    async fn load_attribute(&self, id: AttributeId) -> Option<AttributeDTO> {
        let path = paths::get_attributes_path().join(id.into_inner().to_string());

        if !path.exists() {
            None
        } else {
            let s = fs::read_to_string(&path).unwrap();
            toml::from_str(&s).unwrap()
        }
    }

    async fn delete_card(&self, id: CardId) {
        let path = paths::get_cards_path().join(id.to_string());
        fs::remove_file(&path).unwrap();
    }

    async fn delete_attribute(&self, id: AttributeId) {
        let path = paths::get_attributes_path().join(id.into_inner().to_string());
        fs::remove_file(&path).unwrap();
    }

    async fn load_reviews(&self, id: CardId) -> Vec<Review> {
        let mut reviews = vec![];
        let path = paths::get_review_path().join(id.to_string());

        if !path.exists() {
            return Default::default();
        }

        let s = read_to_string(&path).unwrap();
        for line in s.lines() {
            let (timestamp, grade) = line.split_once(' ').unwrap();
            let timestamp = Duration::from_secs(timestamp.parse().unwrap());
            let grade = Recall::from_str(grade).unwrap();
            let review = Review {
                timestamp,
                grade,
                time_spent: Duration::default(),
            };
            reviews.push(review);
        }

        reviews.sort_by_key(|r| r.timestamp);
        reviews
    }

    async fn save_reviews(&self, id: CardId, reviews: Vec<Review>) {
        let path = paths::get_review_path().join(id.to_string());
        let mut s = String::new();
        for r in reviews {
            let stamp = r.timestamp.as_secs().to_string();
            let grade = match r.grade {
                Recall::None => "1",
                Recall::Late => "2",
                Recall::Some => "3",
                Recall::Perfect => "4",
            };
            s.push_str(&format!("{} {}\n", stamp, grade));
        }

        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&mut s.as_bytes()).unwrap();
    }

    async fn load_config(&self) -> Config {
        Config
    }

    async fn save_config(&self, _config: Config) {
        todo!()
    }
}
