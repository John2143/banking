use std::{net::SocketAddr, sync::Arc};

use aide::openapi::{Info, OpenApi};
use anyhow::Context;
use axum::Extension;
use clap::Parser;
use schemars::JsonSchema;
use sqlx::{PgPool, postgres::PgPoolOptions};

use tokio::time::Duration;
use tower_http::services::ServeDir;

pub mod api;
pub mod utils;

use utils::serve_file;

#[derive(Debug, Clone, clap::Parser)]
pub struct Config {
    #[clap(long, default_value = "3000")]
    pub port: u16,
    //#[clap(long, short, env)]
    //pub database_url: String,
    #[clap(long, env, default_value = "info")]
    pub rust_log: String,

    #[clap(
        long,
        env,
        default_value = "postgres://postgres@localhost:5432/stasher"
    )]
    pub database_url: String,

    #[clap(long, env, default_value = "./frontend")]
    pub frontend_folder: String,

    #[clap(long, env, short = 'd')]
    pub disable_tasks: Vec<ActiveTasks>,

    #[clap(long, env, short = 'e')]
    pub extra_tasks: Vec<ActiveTasks>,
}

impl Config {
}

#[derive(
    Debug, Clone, serde::Deserialize, serde::Serialize, JsonSchema, clap::ValueEnum, PartialEq, Eq,
)]
/// Disable these tasks by passing them with the `-d` flag
pub enum ActiveTasks {
    All,
    // Read the current public lobbies and look for new games
    //SomeExampleTask,
}

/// Spawn the background worker tasks
async fn launch_tasks(config: Arc<Config>, database: PgPool) -> anyhow::Result<()> {
    if config.disable_tasks.contains(&ActiveTasks::All) {
        tracing::info!("All tasks are disabled, skipping task launch");
        return Ok(());
    }

    //if config
        //.extra_tasks
        //.contains(&ActiveTasks::SomeExampleTask)
    //{
        //let db = database.clone();
        //let cfg = config.clone();
        //keep_task_alive(
            //move || task(db.clone(), cfg.clone()),
            //TaskSettings {
                //sleep_time: Duration::from_secs(60),
                //..Default::default()
            //},
        //);
    //}

    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();
    tracing_subscriber::fmt()
        //.with_max_level(tracing::Level::INFO)
        .with_env_filter(&config.rust_log)
        .with_target(false)
        .without_time()
        .init();

    tracing::info!("Starting api...");

    let database = PgPoolOptions::new()
        .min_connections(2)
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&config.database_url)
        .await
        .context("Failed to create database connection pool")?;

    let config = std::sync::Arc::new(config);

    let db = database.clone();
    tokio::spawn(async move {
        match sqlx::migrate!("./migrations").run(&db).await {
            Ok(_) => tracing::info!("Database migrations applied successfully"),
            Err(e) => tracing::error!("Failed to apply database migrations: {}", e),
        }
    });

    // TODO: Make sure we don't have vulnerabilites around the frontend because CORS.
    // (especially around oauth stuff)
    //
    // Let other people freely call this API for their own frontends, as they nicely set a
    // user-agent to distinguish their apps
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(vec![
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any);

    let mut openapi = OpenApi {
        info: Info {
            title: "stasher api".to_string(),
            version: "1.0.0".to_string(),
            description: Some(
                "This API can be used to interact with your upcoming and past investments."
                    .to_string(),
            ),
            ..Default::default()
        },
        ..Default::default()
    };

    let routes = api::routes(openapi.clone(), cors).layer(Extension(database.clone()));

    // If we don't have a frontend folder then use this as a
    // minimal fallback.
    fn default_missing_response() -> axum::response::Response {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Not Found"))
            .expect("Failed to build 404 response")
    }

    // just serve the SPA?
    let missing_html = format!("{}/index.html", config.frontend_folder);

    let index_html = axum::routing::get(|| async move {
        serve_file(std::path::Path::new(&missing_html))
            .await
            .unwrap_or(default_missing_response())
    });

    let fin = routes
        // Frontend routes
        .route("/", index_html.clone())
        .finish_api(&mut openapi)
        .layer(Extension(openapi.clone()))
        .layer(Extension(config.clone()))
        .layer(
            // TODO Figure out how to embed a "request_id" without a lot of boilerplate so that we
            // can tie the request and response together in the logs.
            tower_http::trace::TraceLayer::new_for_http()
        )
        // I'm not sure if this is working - /api/v1/lobbies/ doesn't work for me
        .layer(tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash())
        // Serve the frontend from a static directory.
        .fallback_service(axum::routing::get_service(
            ServeDir::new(&*config.frontend_folder)
                .append_index_html_on_directories(true)
                // 404 Service
                .not_found_service(index_html),
        ));



    // This launches db async tasks.
    launch_tasks(config.clone(), database.clone())
        .await
        .context("Failed to launch async tasks")?;

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!("HTTP Server Listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        fin.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    unreachable!("Server stopped unexpectedly");
}

