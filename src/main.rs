use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rand::distr::{Alphanumeric, SampleString};
use std::{env, io::ErrorKind, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
struct Config {
    /// The address the server will bind to, e.g., "127.0.0.1:8080".
    addr: String,
    /// Where paste files get stored. Keep an eye on size since there's currently no cleanup.
    data_dir: String,
    /// The base URL for fetching pastes. Distinct from `addr` if eg behind a reverse proxy.
    base_url: String,
}

fn get_config() -> Config {
    let addr = env::var("QPASTE_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    // consider also accepting a pg db connection - fewer potential security vulns than storing files on disk
    let data_dir = env::var("QPASTE_DATA_DIR").expect("QPASTE_DATA_DIR must be set to store files");
    // ensure data_dir exists
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    let base_url = env::var("QPASTE_BASE_URL").unwrap_or_else(|_| format!("http://{}", addr));
    Config {
        addr,
        base_url,
        data_dir,
    }
}

#[tokio::main]
async fn main() {
    let config = get_config();

    let listener = tokio::net::TcpListener::bind(&config.addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("listening on http://{local_addr}");

    axum::serve(listener, app(Arc::new(config)))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn app(config: Arc<Config>) -> Router {
    Router::new()
        .route("/", post(accept_form))
        .route("/f/{file_id}", get(serve_file))
        // https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html#difference-between-defaultbodylimit-and-requestbodylimit
        // short version seems to be that DefaultBodyLimit applies to individual "extractors"
        // (like the Multipart extractor in the accept function) while RequestBodyLimitLayer
        // applies to the request as part of the middleware pipeline instead.
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024 /* 10MB */))
        // .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(config)
}

async fn accept_form(
    State(config): State<Arc<Config>>,
    mut multipart: Multipart,
) -> Result<String, (StatusCode, String)> {
    let mut ids = Vec::new();

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
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        while let Some(v) = field
            .chunk()
            .await
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
        {
            file.write_all(&v)
                .await
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        }

        println!("Wrote file with ID {}", id);
        ids.push(id);
    }

    Ok(ids
        .iter()
        .map(|id| format!("{}/f/{}", config.base_url, id))
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn serve_file(
    State(config): State<Arc<Config>>,
    Path(file_id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    if !validate_file_id(&file_id) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid file ID".to_string(),
        ));
    }

    let file_path = std::path::Path::new(&config.data_dir).join(&file_id);

    let file = match tokio::fs::File::open(&file_path).await {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            Err((StatusCode::NOT_FOUND, "File not found".to_string()))?
        }
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to open file: {}", err),
        ))?,
    };

    let len = file
        .metadata()
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get file metadata: {}", err),
            )
        })?
        .len();

    let body = Body::from_stream(ReaderStream::new(file));

    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::CONTENT_DISPOSITION, "attachment".to_string()),
        ],
        body,
    )
        .into_response())
}

fn validate_file_id(file_id: &str) -> bool {
    file_id.len() == 7
        && file_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}
