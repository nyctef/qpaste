use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use rand::distr::{Alphanumeric, SampleString};
use regex;
use std::env;
use tokio::io::AsyncWriteExt;
use tower_http::limit::RequestBodyLimitLayer;

fn get_config() -> Config {
    let addr = env::var("QPASTE_ADDR").unwrap_or("127.0.0.1:3000".to_string());
    // consider also accepting a pg db connection - fewer potential security vulns than storing files on disk
    let data_dir = env::var("QPASTE_DATA_DIR").expect("QPASTE_DATA_DIR must be set to store files");
    // ensure data_dir exists
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    Config { data_dir, addr }
}

#[tokio::main]
async fn main() {
    let config = get_config();

    let app = Router::new()
        .route("/", post(accept_form))
        .route("/f/{file_id}", get(serve_file))
        // https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html#difference-between-defaultbodylimit-and-requestbodylimit
        // short version seems to be that DefaultBodyLimit applies to individual "extractors"
        // (like the Multipart extractor in the accept function) while RequestBodyLimitLayer
        // applies to the request as part of the middleware pipeline instead.
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024 /* 10MB */))
        // .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(config.clone());

    let listener = tokio::net::TcpListener::bind(&config.addr).await.unwrap();
    // tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[derive(Clone)]
struct Config {
    data_dir: String,
    addr: String,
}

async fn accept_form(
    State(config): State<Config>,
    mut multipart: Multipart,
) -> Result<(), (StatusCode, String)> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    {
        let id = Alphanumeric.sample_string(&mut rand::rng(), 6);
        let id = format!("{}-{}", &id[0..3], &id[3..6]);

        let file_path = std::path::Path::new(&config.data_dir).join(&id);
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .await
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        while let Some(v) = field
            .chunk()
            .await
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
        {
            file.write_all(&v)
                .await
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        }

        println!("Wrote file with ID {}", &id);
    }
    Ok(())
}

async fn serve_file(
    State(config): State<Config>,
    Path(file_id): Path<String>,
) -> impl IntoResponse {
    let file_id_re = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();

    if !file_id_re.is_match(&file_id) {
        return (axum::http::StatusCode::BAD_REQUEST, "Invalid file ID").into_response();
    }

    let file_path = std::path::Path::new(&config.data_dir).join(&file_id);

    if !file_path.exists() {
        return (axum::http::StatusCode::NOT_FOUND, "File not found").into_response();
    }

    match tokio::fs::read(&file_path).await {
        Ok(contents) => (axum::http::StatusCode::OK, contents).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read file",
        )
            .into_response(),
    }
}
