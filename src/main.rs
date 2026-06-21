use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    // ponytail: read DATABASE_URL only to fail fast if .env isn't sourced;
    // real pool wiring comes in M3.
    std::env::var("DATABASE_URL").expect("DATABASE_URL missing — .env not loaded?");

    let app = Router::new().route("/", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
