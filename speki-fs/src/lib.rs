use rayon::prelude::*;
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

impl SpekiProvider for FileProvider {
    fn load_all_cards() -> Vec<RawCard> {
        load_files(paths::get_cards_path())
            .unwrap()
            .into_par_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    fn save_card(card: RawCard) {
        let s: String = toml::to_string(&card).unwrap();
        let path = paths::get_cards_path().join(card.id.to_string());
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
    }

    fn load_card(id: CardId) -> Option<RawCard> {
        let path = paths::get_cards_path().join(id.to_string());

        if !path.exists() {
            None
        } else {
            let s = fs::read_to_string(&path).unwrap();
            toml::from_str(&s).unwrap()
        }
    }

    fn load_all_attributes() -> Vec<AttributeDTO> {
        load_files(paths::get_attributes_path())
            .unwrap()
            .into_par_iter()
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    fn save_attribute(attribute: AttributeDTO) {
        let s: String = toml::to_string(&attribute).unwrap();
        let path = paths::get_attributes_path().join(attribute.id.into_inner().to_string());
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
    }

    fn load_attribute(id: AttributeId) -> Option<AttributeDTO> {
        let path = paths::get_attributes_path().join(id.into_inner().to_string());

        if !path.exists() {
            None
        } else {
            let s = fs::read_to_string(&path).unwrap();
            toml::from_str(&s).unwrap()
        }
    }

    fn delete_card(id: CardId) {
        let path = paths::get_cards_path().join(id.to_string());
        fs::remove_file(&path).unwrap();
    }

    fn delete_attribute(id: AttributeId) {
        let path = paths::get_attributes_path().join(id.into_inner().to_string());
        fs::remove_file(&path).unwrap();
    }

    fn load_reviews(id: CardId) -> Vec<Review> {
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

    fn save_reviews(id: CardId, reviews: Vec<Review>) {
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

        Self::load_reviews(id);
    }
}
