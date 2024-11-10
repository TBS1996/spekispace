#![allow(non_snake_case)]

use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use dioxus_logger::tracing::{Level, info};
use serde::{ Serialize};
use web_sys::console;
use wasm_bindgen_futures::spawn_local;



pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}


#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/callback?:code&:state")] 
    CallBack {
        code: String,
        state: String,
    },
}


fn main() {
    // Init logger
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    info!("starting app");
    launch(App);
}


fn App() -> Element {
    use_context_provider(State::new);
    rsx! {
        Router::<Route> {}
    }
}


#[derive(Clone, Default)]
pub struct State {
    inner: Arc<Mutex<InnerState>>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn logged_in(&self) -> Signal<bool> {
        self.inner.lock().unwrap().logged_in.clone()
    }
}

#[derive(Default)]
struct InnerState {
    logged_in: Signal<bool>,
}



#[component]
fn CallBack(code: ReadOnlySignal<String>, state: ReadOnlySignal<String>) -> Element {
    let code_value = code().clone();
    let state_value = state().clone();



    rsx!{
        h1 {"code: {code}, state: {state}"}



        button { onclick: move |_| {
            let code = code_value.clone();
            let state = state_value.clone();

            send_auth_request(code, state);

        }
        , "send to backend :D" }

    }
}

fn send_auth_request(code: String, state: String) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::spawn_local;
    use web_sys::{Request, RequestInit, RequestMode, Response};
    use serde::Serialize;
    use serde_json;

    #[derive(Serialize)]
    struct AuthParams {
        code: String,
        state: String,
    }

    spawn_local(async move {
        // Create the AuthParams struct and convert it to JSON string for the request body
        let auth_params = AuthParams { code, state };
        let auth_params_json = serde_json::to_string(&auth_params).unwrap(); // Serialize to JSON string
        let auth_params_js = JsValue::from_str(&auth_params_json); // Convert JSON string to JsValue

        // Configure the request with method, mode, and body
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);
        opts.set_body(&auth_params_js);

        // Create the request with URL and options
        let request = Request::new_with_str_and_init("http://localhost:3000/auth/github/callback", &opts).unwrap();
        request.headers().set("Content-Type", "application/json").unwrap();

        // Send the request
        let window = web_sys::window().unwrap();
        let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await.unwrap();
        let resp: Response = resp_value.dyn_into().unwrap();

        // Handle the JSON response
        if resp.ok() {
            let json = wasm_bindgen_futures::JsFuture::from(resp.json().unwrap()).await.unwrap();
            log_to_console(format!("Access token received: {:?}", json));
        } else {
            let error_text = wasm_bindgen_futures::JsFuture::from(resp.text().unwrap()).await.unwrap();
            log_to_console(format!("Error: {:?}", error_text));
        }
    });
}



#[component]
fn Home() -> Element {
    let state = use_context::<State>();
    let mut flag = state.logged_in();

    rsx!{
        if flag() {
            "log out"
        }else {
            "log in"
        }

        button { onclick: move |_| {flag.set(true);

            spawn_local(async move {
                    let auth_url = "http://localhost:3000/auth/github";
                    let x = web_sys::window().unwrap().location().set_href(auth_url).unwrap();
                    log_to_console(x);
            });

        }, "log in" }
        button { onclick: move |_| flag.set(false), "log out" }
    }
}



/* 
#[derive(Debug, Deserialize, Serialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
}


#[derive(Serialize)]
struct AuthParams {
    code: String,
    state: String,
}


#[wasm_bindgen]
pub async fn poll_for_token(device_code: String, interval: u32) -> Result<JsValue, JsValue> {
    loop {
        let mut opts = RequestInit::new();
        opts.method("POST");
        opts.mode(RequestMode::Cors);

        let body = serde_json::json!({
            "client_id": CLIENT_ID,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        });

        opts.body(Some(&JsValue::from_str(&body.to_string())));

        // Create and send the request
        let request = Request::new_with_str_and_init("https://github.com/login/oauth/access_token", &opts)?;
        request.headers().set("Accept", "application/json")?;

        let window = web_sys::window().ok_or("No global `window` exists")?;
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;

        // Convert the response to `web_sys::Response`
        let resp: Response = resp_value.dyn_into()?;

        // Parse the response JSON if successful
        if resp.ok() {
            let json = JsFuture::from(resp.json()?).await?;
            let token: AccessTokenResponse = json.into_serde().unwrap();
            return Ok(JsValue::from_serde(&token).unwrap());
        } else {
            // Wait for the specified interval before retrying
            TimeoutFuture::new(interval * 1000).await;
        }
    }
}




#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoginInfo {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub login: String,
    pub id: u64,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
}



// Call this function to log a message
pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}

impl LoginInfo {
    fn new(token: AccessTokenResponse, login: GitHubUser) -> Self {
        Self {
            access_token: token.access_token,
            token_type: token.token_type,
            scope: token.scope,
            login: login.login,
            id: login.id,
            name: login.name,
            avatar_url: login.avatar_url,
            html_url: login.html_url,
        }
    }
}



#[derive(Deserialize, Debug)]
struct GitHubUser {
    login: String,
    id: u64,
    name: Option<String>,
    avatar_url: String,
    html_url: String,
}



#[derive(Deserialize, Debug, Serialize)]
pub struct DeviceResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u32,
}




pub async fn request_device_code() -> Result<DeviceResponse, String> {
    // Configure the request
    let mut opts = RequestInit::new();
    opts.method("POST");
    opts.mode(RequestMode::Cors); // Use CORS mode to allow cross-origin requests

    // Create the form data for `client_id` and `scope`
    let form_data = format!("client_id={}&scope=repo", CLIENT_ID);
    opts.body(Some(&wasm_bindgen::JsValue::from_str(&form_data)));

    // Initialize the request
    let request = Request::new_with_str_and_init("https://github.com/login/device/code", &opts)
        .map_err(|err| format!("Failed to create request: {:?}", err))?;
    request.headers().set("Accept", "application/json").unwrap();
    request
        .headers()
        .set("Content-Type", "application/x-www-form-urlencoded")
        .unwrap();

    // Send the request and await the response
    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|err| format!("Failed to fetch: {:?}", err))?;

    // Convert the response to a `Response` object
    let resp: Response = resp_value.dyn_into().unwrap();

    // Check if the response is successful
    if resp.ok() {
        let json = JsFuture::from(resp.json().unwrap())
            .await
            .map_err(|err| format!("Failed to parse JSON: {:?}", err))?;
        let device_response: DeviceResponse =
            json.into_serde().map_err(|err| format!("Failed to deserialize: {:?}", err))?;
        Ok(device_response)
    } else {
        Err(format!("Failed to request device code: {:?}", resp.status_text()))
    }
}

*/