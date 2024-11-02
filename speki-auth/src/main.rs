use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use axum::{extract::Query, response::IntoResponse, routing::get, Router};
use oauth2::reqwest::async_http_client;
use oauth2::{basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, RedirectUrl, TokenUrl};
use oauth2::{CsrfToken, PkceCodeVerifier, TokenResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use chrono::Utc;

const CLIENT_ID: &'static str = "Ov23lihX6Mhl07qzP1Yh";
const REDIRECT_URI: &'static str = "http://localhost:3000/auth/github/callback";
const APP_ID: &'static str = "1044773";
const INSTALLATION_ID: &'static str = "56713615";
const PRIVATE_KEY: &'static str = include_str!("/home/tor/Downloads/spekipem.pem");





#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iat: usize,  
    exp: usize,  
    iss: String, 
}


fn generate_jwt(app_id: &str, private_key: &str) -> String {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        iat: now,
        exp: now + (10 * 60), // Token valid for 10 minutes
        iss: app_id.to_string(),
    };

    encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(private_key.as_bytes()).expect("Invalid private key"),
    )
    .expect("JWT generation failed")
}

async fn get_installation_access_token(jwt: &str, installation_id: &str) -> Result<String, reqwest::Error> {
    let url = format!(
        "https://api.github.com/app/installations/{}/access_tokens",
        installation_id
    );
    dbg!(&url);
    dbg!(&jwt);

    let client = Client::new();
    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "speki-app") // Set the User-Agent header
        .send()
        .await;

    dbg!(&res);

    let res = res?;

    if !res.status().is_success() {
        dbg!(res.text().await);
        panic!();
    }




    let json: serde_json::Value = res.json().await?;
    dbg!(&json);
    Ok(json["token"].as_str().unwrap().to_string())
}




async fn flow() -> String{
    let jwt = generate_jwt(APP_ID, PRIVATE_KEY);
    get_installation_access_token(&jwt, &INSTALLATION_ID).await.unwrap()

}


use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;

async fn list_repos(Extension(github_app): Extension<Arc<GitHubApp>>) -> impl IntoResponse {
    let jwt = generate_jwt(&github_app.app_id, &github_app.private_key);

    let token = match get_installation_access_token(&jwt, &github_app.installation_id).await {
        Ok(token) => token,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get installation token").into_response(),
    };

    let url = "https://api.github.com/installation/repositories";
    let client = Client::new();
    let res = client
        .get(url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "speki-app") 
        .send()
        .await;


    match res {
        Ok(response) if response.status().is_success() => {
            let repos: serde_json::Value = response.json().await.unwrap();

            let obj = repos.as_object().unwrap();
            for keyval in obj.iter() {
                dbg!(keyval.0);
            }



            let mut repo_map: HashMap<String, serde_json::Value> = HashMap::default();
            let repos = obj.get("repositories").unwrap().as_array().unwrap();

            for repo in repos.into_iter() {
                let name = repo.as_object().unwrap().get("full_name").unwrap();
                let name = name.to_string();
                repo_map.insert(name, repo.clone());

            }


            (StatusCode::OK, "howdy".to_string()).into_response()
        }
        Ok(res) => {
            dbg!(res.text().await);
            panic!();
            
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list repositories").into_response(),
    }
}


#[derive(Clone)]
struct GitHubApp {
    app_id: String,
    installation_id: String,
    private_key: String,
}



#[tokio::main]
async fn main() {
    let verifier_store = VerifierStore::new();

    let ga = GitHubApp {
        app_id: APP_ID.to_string(),
        installation_id: INSTALLATION_ID.to_string(),
        private_key: PRIVATE_KEY.to_string(),
    };

    let app = Router::new()
        .route("/auth/github", get(github_login))
        .route("/auth/github/callback", get(github_callback))
        .route("/github/repos", get(list_repos))
        .layer(Extension(verifier_store))
        .layer(Extension(Arc::new(ga)));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Server running at http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Clone, Default)]
struct VerifierStore {
    inner: Arc<Mutex<HashMap<String, PkceCodeVerifier>>>,
}

impl VerifierStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, csrf_token: CsrfToken, verifier: PkceCodeVerifier) {
        let token = csrf_token.secret().to_owned();
        let mut store = self.inner.lock().unwrap();
        store.insert(token, verifier);
    }

    fn take(&self, csrf_token: &CsrfToken) -> Option<PkceCodeVerifier> {
        let token = csrf_token.secret();
        let mut store = self.inner.lock().unwrap();
        store.remove(token)
    }
}

fn oauth_client() -> BasicClient {
    let client_id = ClientId::new(CLIENT_ID.to_string());
    let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
        .expect("Invalid token endpoint URL");

    BasicClient::new(client_id, None, auth_url, Some(token_url))
 //       .set_redirect_uri(RedirectUrl::new(REDIRECT_URI.to_string()).unwrap()
}

async fn github_login(Extension(verifier_store): Extension<VerifierStore>) -> impl IntoResponse {
    let (challenge, verifier) = oauth2::PkceCodeChallenge::new_random_sha256();

    let csrf = oauth2::CsrfToken::new_random();

    // Generate the authorization URL using the oauth2 crate
    let (auth_url, _csrf_token) = oauth_client()
        .authorize_url(|| csrf.clone())
        .set_pkce_challenge(challenge)
        .url();


    dbg!(&auth_url);

    verifier_store.insert(csrf, verifier);

    (
        StatusCode::FOUND,
        [(axum::http::header::LOCATION, auth_url.to_string())],
    )
}

#[derive(Deserialize)]
struct AuthQuery {
    code: String,
    state: CsrfToken,
}

async fn github_callback(
    Query(params): Query<AuthQuery>,
    Extension(verifier_store): Extension<VerifierStore>,
) -> impl IntoResponse {
    dbg!("callbcak timeee");
    let csrf_token = params.state;
    let pkce_verifier = match verifier_store.take(&csrf_token) {
        Some(verifier) => verifier,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid or missing PKCE verifier"})),
            )
        }
    };

    let token_result = oauth_client()
        .exchange_code(AuthorizationCode::new(params.code))
        .set_pkce_verifier(pkce_verifier) // Set the PKCE verifier here
        .request_async(async_http_client)
        .await;

    match token_result {
        Ok(token) => {
            let access_token = token.access_token().secret();
            let token = Json(json!({"access_token": access_token}));
            dbg!(&token);
            (StatusCode::OK, token)
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to get access token: {:?}", err)})),
        ),
    }
}
