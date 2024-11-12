use axum::http::HeaderValue;
use axum::http::Response;
use axum::http::StatusCode;
use axum::Extension;
use axum::{extract::Query, response::IntoResponse, routing::get, Router};
use oauth2::reqwest::async_http_client;
use oauth2::ClientSecret;
use oauth2::{basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, RedirectUrl, TokenUrl};
use oauth2::{CsrfToken, TokenResponse};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

const CLIENT_ID: &'static str = "Iv23lipnBB1I2i58tzVT";
const REDIRECT_URI: &'static str = "http://localhost:3000/auth/github/callback";
const APP_ID: &'static str = "1044773";
const INSTALLATION_ID: &'static str = "56713615";
const PRIVATE_KEY: &'static str = include_str!("/home/tor/prog/privkey.pem");
const CLIENT_SECRET: &'static str = include_str!("/home/tor/prog/client_secret");

async fn get_installation_token() -> String {
    let jwt = generate_jwt().await;

    let client = Client::new();
    let url = format!(
        "https://api.github.com/app/installations/{}/access_tokens",
        INSTALLATION_ID
    );

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "speki-auth")
        .send()
        .await
        .unwrap();

    #[derive(Deserialize)]
    struct TokenResponse {
        token: String,
    }

    let res: TokenResponse = response.json().await.unwrap();
    res.token
}

async fn generate_jwt() -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Serialize)]
    struct Claims {
        iat: usize,
        exp: usize,
        iss: String,
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;
    let claims = Claims {
        iat: now,
        exp: now + 600,
        iss: APP_ID.to_string(),
    };

    let key = EncodingKey::from_rsa_pem(PRIVATE_KEY.as_bytes()).unwrap();
    let token = encode(&Header::new(Algorithm::RS256), &claims, &key).unwrap();

    token
}

mod other;

use other::list_repos;

#[derive(Clone)]
pub struct GitHubApp {
    app_id: String,
    installation_id: String,
    private_key: String,
}

async fn fallback_route(
    uri: axum::http::Uri,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    dbg!(&uri.path());
    dbg!(&uri.path_and_query());

    dbg!("Fallback route hit! URI: {}", uri);
    dbg!("Query params: {:?}", params);

    (StatusCode::OK, "fallback lol")
}

use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    let verifier_store = VerifierStore::new();

    let ga = GitHubApp {
        app_id: APP_ID.to_string(),
        installation_id: INSTALLATION_ID.to_string(),
        private_key: PRIVATE_KEY.to_string(),
    };

    let cors = CorsLayer::new()
        .allow_origin("http://localhost:8080".parse::<HeaderValue>().unwrap())
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/auth/github", get(github_login))
        .route("/auth/github/callback", get(github_callback))
        .route("/github/repos", get(list_repos))
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .fallback(axum::routing::any(fallback_route))
        .layer(Extension(verifier_store))
        .layer(Extension(Arc::new(ga)))
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Server running at http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Clone, Default)]
struct VerifierStore {
    inner: Arc<Mutex<HashSet<String>>>,
}

impl VerifierStore {
    pub fn new() -> Self {
        Self::default()
    }

    // Insert a new CSRF token
    fn insert(&self, csrf_token: CsrfToken) {
        let token = csrf_token.secret().to_owned();
        let mut store = self.inner.lock().unwrap();
        store.insert(token);
    }

    // Check for and remove a CSRF token (returns true if it was present)
    fn take(&self, csrf_token: &CsrfToken) -> bool {
        let token = csrf_token.secret();
        let mut store = self.inner.lock().unwrap();
        store.remove(token)
    }
}

fn oauth_client() -> BasicClient {
    let client_id = ClientId::new(CLIENT_ID.to_string());
    let client_secret = Some(ClientSecret::new(dbg!(CLIENT_SECRET.trim().to_string())));
    let redirect_uri = RedirectUrl::new(REDIRECT_URI.trim().to_string()).unwrap();

    let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
        .expect("Invalid token endpoint URL");

    BasicClient::new(client_id, client_secret, auth_url, Some(token_url))
        .set_redirect_uri(redirect_uri)
}

async fn github_login(Extension(verifier_store): Extension<VerifierStore>) -> impl IntoResponse {
    let csrf = oauth2::CsrfToken::new_random();
    let (auth_url, _csrf_token) = oauth_client().authorize_url(|| csrf.clone()).url();
    verifier_store.insert(csrf);

    (
        StatusCode::FOUND,
        [(axum::http::header::LOCATION, auth_url.to_string())],
    )
}

#[derive(Deserialize, Debug)]
struct AuthQuery {
    code: String,
    state: CsrfToken,
}

#[axum::debug_handler]
async fn github_callback(
    Extension(verifier_store): Extension<VerifierStore>,
    Query(params): Query<AuthQuery>,
) -> impl IntoResponse {
    dbg!("@@@@@@@@@@@@@@@@@");

    use axum::body::Body;

    let csrf_token = params.state;

    // Verify the CSRF token
    if !verifier_store.take(&csrf_token) {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("state not found"))
            .unwrap();
    }

    // Exchange the authorization code for an access token
    let token_result = oauth_client()
        .exchange_code(AuthorizationCode::new(params.code))
        .request_async(async_http_client)
        .await;

    match token_result {
        Ok(token) => {
            let access_token = token.access_token().secret().to_string();
            use axum_extra::extract::cookie::Cookie;
            let mut cookie = Cookie::new("auth-token", access_token);
            cookie.set_http_only(false);
            cookie.set_secure(false);
            cookie.set_path("/");

            let install_token = get_installation_token().await;
            let mut cookie2 = Cookie::new("install-token", install_token);
            cookie2.set_http_only(false);
            cookie2.set_secure(false);
            cookie2.set_path("/");

            dbg!(Response::builder()
                .status(StatusCode::FOUND)
                .header(
                    "Set-Cookie",
                    HeaderValue::from_str(&cookie.to_string()).unwrap(),
                )
                .header(
                    "Set-Cookie",
                    HeaderValue::from_str(&cookie2.to_string()).unwrap(),
                )
                .header(
                    "Location",
                    HeaderValue::from_static("http://localhost:8080/")
                )
                .body("Redirecting to homepage".into())
                .unwrap())
        }
        Err(_err) => dbg!(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body("lol".into())
            .unwrap()),
    }
}
