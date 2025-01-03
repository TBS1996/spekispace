use std::net::SocketAddr;

use axum::{
    body::Body,
    http::{HeaderValue, Request, Response, StatusCode},
    routing::any,
    Router,
};
use hyper::{client::HttpConnector, header::HOST, HeaderMap, Method};
use hyper_tls::HttpsConnector;

#[tokio::main]
async fn main() {
    let https = HttpsConnector::new();
    let client = hyper::Client::builder().build::<_, Body>(https);

    let app = Router::new().route(
        "/*path",
        any(move |req| handle_request(req, client.clone())),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    println!("Listening on http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn handle_preflight(req: &Request<Body>) -> Result<Response<Body>, StatusCode> {
    dbg!("HANDLE PREFLLIGHt");
    dbg!(&req);
    let mut response = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type, Authorization"),
    );

    Ok(response)
}

async fn handle_request(
    req: Request<Body>,
    client: hyper::Client<HttpsConnector<HttpConnector>>,
) -> Result<Response<Body>, StatusCode> {
    if req.method() == Method::OPTIONS {
        return handle_preflight(&req);
    }

    proxy(req, client).await
}

fn get_redirect_location(headers: &HeaderMap) -> Option<String> {
    headers
        .get("location")
        .and_then(|loc| loc.to_str().ok())
        .map(String::from)
}

async fn proxy(
    mut req: Request<Body>,
    client: hyper::Client<HttpsConnector<HttpConnector>>,
) -> Result<Response<Body>, StatusCode> {
    use hyper::{header::HeaderValue, Body, Request, StatusCode, Uri};

    req.headers_mut()
        .insert(HOST, HeaderValue::from_static("github.com"));

    let origin = req
        .headers()
        .get("Origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("*")
        .to_string();

    let original_method = req.method().clone();

    let uri = match req.uri().path_and_query() {
        Some(pq) => format!("https:/{}", pq),
        None => panic!(),
    };

    dbg!(&uri);

    *req.uri_mut() = Uri::try_from(uri).map_err(|_| {
        dbg!("Bad Request URI");
        StatusCode::BAD_REQUEST
    })?;

    let mut response = client.request(req).await.map_err(|err| {
        dbg!(format!("Error forwarding request: {:?}", err));
        StatusCode::BAD_GATEWAY
    })?;

    for _ in 0..5 {
        if let Some(location) = get_redirect_location(response.headers()) {
            let new_uri = Uri::try_from(location).map_err(|_| {
                dbg!("Invalid redirect location");
                StatusCode::BAD_GATEWAY
            })?;

            let mut req = Request::builder()
                .uri(new_uri)
                .method(original_method.clone())
                .body(Body::empty())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            req.headers_mut()
                .insert(HOST, HeaderValue::from_static("github.com"));

            response = match client.request(req).await {
                Ok(res) => res,
                Err(err) => {
                    dbg!(format!("Error following redirect: {:?}", err));
                    return Err(StatusCode::BAD_GATEWAY);
                }
            };
        } else {
            break;
        }
    }

    response.headers_mut().insert(
        "Access-Control-Allow-Origin",
        HeaderValue::from_str(&origin).unwrap_or(HeaderValue::from_static("*")),
    );
    response.headers_mut().insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response.headers_mut().insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type"),
    );

    response
        .headers_mut()
        .insert("Connection", HeaderValue::from_static("close"));

    Ok(response)
}
