use std::{collections::HashMap, sync::Arc};

use axum::{http::StatusCode, response::IntoResponse, Extension};
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iat: usize,
    exp: usize,
    iss: String,
}

pub async fn list_repos(Extension(github_app): Extension<Arc<GitHubApp>>) -> impl IntoResponse {
    let jwt = generate_jwt(&github_app.app_id, &github_app.private_key);

    let token = match get_installation_access_token(&jwt, &github_app.installation_id).await {
        Ok(token) => token,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get installation token",
            )
                .into_response()
        }
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
        Ok(_) => {
            panic!();
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list repositories",
        )
            .into_response(),
    }
}

async fn get_installation_access_token(
    jwt: &str,
    installation_id: &str,
) -> Result<String, reqwest::Error> {
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
        panic!();
    }

    let json: serde_json::Value = res.json().await?;
    dbg!(&json);
    Ok(json["token"].as_str().unwrap().to_string())
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
