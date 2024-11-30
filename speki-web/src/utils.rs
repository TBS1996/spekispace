use dioxus::hooks::use_context;

use crate::{
    js,
    login::{LoginState, UserInfo},
    PROXY, REPO_PATH,
};

pub mod cookies {
    use std::collections::HashMap;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = "
export function getCookies() {
    const cookies = document.cookie;
    console.log('Cookies:', cookies);
    return cookies;
}
")]
    extern "C" {
        fn getCookies() -> String;
    }

    pub fn get(key: &str) -> Option<String> {
        parse_cookies(&getCookies()).get(key).cloned()
    }

    fn parse_cookies(cookie_header: &str) -> HashMap<String, String> {
        cookie_header
            .split("; ")
            .filter_map(|cookie| {
                let parts: Vec<&str> = cookie.split('=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    }
}

pub fn get_install_token() -> Option<String> {
    cookies::get("install-token")
}

pub fn get_auth_token() -> Option<String> {
    cookies::get("auth-token")
}

pub fn sync_repo() {
    let info = use_context::<LoginState>();

    if let Some(token) = info.auth_token() {
        js::sync_repo(REPO_PATH, &token, PROXY);
    }
}
