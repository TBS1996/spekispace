use std::sync::Arc;

use dioxus::{html::FileEngine, prelude::*};
use speki_core::audio::Audio;
use tracing::info;

use crate::pages::play_audio;

#[component]
pub fn AudioUpload(audio: Signal<Option<Audio>>) -> Element {
    let read_file = move |file_engine: Arc<dyn FileEngine>, file_name: String| async move {
        if let Some(file_content) = file_engine.read_file(&file_name).await {
            info!("file size: {}", file_content.len());
            info!(
                "Audio data (first 20 bytes): {:?}",
                &file_content[..20.min(file_content.len())]
            );
            audio.clone().set(Some(Audio::new(file_content)));
        }
    };

    let upload_file = move |evt: FormEvent| async move {
        if let Some(file_engine) = evt.files() {
            if let Some(file_name) = file_engine.files().get(0) {
                read_file(file_engine, file_name.clone()).await;
            }
        }
    };

    rsx! {
        div {
            match audio.cloned() {
                Some(aud) => {
                    rsx! {
                    button {
                        onclick: move |_| {
                            play_audio(aud.data.clone(), "audio/mpeg");
                        },
                        "play"
                    }
                    button {
                        onclick: move |_| {
                            audio.clone().set(None);
                        },
                        "X"
                    }
                }
                },
                None => rsx! {

            div {
                class: "mt-8",
                label { r#for: "textreader"}
                input {
                    r#type: "file",
                    accept: "audio/*",
                    multiple: false,
                    name: "textreader",
                    directory: false,
                    onchange: upload_file,
                }
            }
                },
            }
        }
    }
}
