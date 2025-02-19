use dioxus::prelude::*;
use tracing::info;

#[component]
pub fn About() -> Element {
    rsx! {
        "welcome!"
    }
}

use web_sys::{Blob, BlobPropertyBag, HtmlAudioElement, Url};

pub fn play_audio(audio_data: &Vec<u8>, mime_type: &str) {
    // Log the MIME type and data size
    info!("Audio data size: {}", audio_data.len());
    info!("MIME type: {}", mime_type);

    // Convert Vec<u8> to Uint8Array

    let array = js_sys::Array::new();
    let uint8arr =
        js_sys::Uint8Array::new(&unsafe { js_sys::Uint8Array::view(audio_data) }.into());
    array.push(&uint8arr.buffer());

    // Create a Blob with the specified MIME type
    let blob_options = BlobPropertyBag::new();
    info!("blob options: {:?}", blob_options);
    blob_options.set_type(mime_type);
    info!("blob options: {:?}", blob_options);
    let blob = Blob::new_with_u8_array_sequence_and_options(&array, &blob_options).unwrap();

    info!("blob type: {}", blob.type_());
    info!("blob as str: {:?}", blob.as_string());
    info!("blob options: {:?}", blob_options);
    info!("blob: {:?}", blob);
    info!("blob size: {:?}", blob.size());

    // Generate a URL for the Blob
    let url = Url::create_object_url_with_blob(&blob).unwrap();
    info!("Generated Blob URL: {}", url);

    // Set up the audio element and play
    let audio = HtmlAudioElement::new_with_src(&url).unwrap();
    spawn(async move {
        info!("audio dur: {}", audio.duration());
        info!("audio dur: {}", audio.src());
        let future = wasm_bindgen_futures::JsFuture::from(audio.play().unwrap()).await;
        info!("some future?? {future:?}");

        match audio.play() {
            Ok(t) => info!("Audio playback started successfully: {t:?}"),
            Err(e) => info!("Audio playback failed: {:?}", e),
        }
    });
}
