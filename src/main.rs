use auth::User;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use clap::Parser;
use env_logger::Env;
use github_app::{CallbackState, GithubApp};

use crate::github_app::WebhookState;

mod auth;
mod github_app;
mod request_logging;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 3000, env)]
    port: u16,

    #[arg(long, default_value = "123456")]
    access_password: String,

    #[arg(long, default_value = "interactions.log")]
    interactions_file: String,

    #[arg(long, default_value = "callback.data")]
    callback_data_file: String,

    #[arg(long, default_value = "webhook.data")]
    webhook_data_file: String,
}

async fn home() -> &'static str {
    "Test GitHub Application"
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::new().filter("LOG_LEVEL").write_style("LOG_STYLE"));

    log::debug!("logging initialized");

    let args = Args::parse();

    log::debug!("args parsed: {:?}", args);

    let logging_state = request_logging::state(&args.interactions_file)
        .await
        .unwrap();

    let allowed_user = User {
        login: "test".to_owned(),
        password: args.access_password.clone(),
    };

    auth::DATABASE.write().await.insert("test".to_owned(), allowed_user);

    let app = Router::new()
        .route(
            "/",
            get(home).layer(middleware::from_fn_with_state("/login".to_owned(), auth::redirect_on_no_auth)),
        )
        .route(
            "/callback",
            get(github_app::callback)
                .with_state(GithubApp::<CallbackState>::new(&args.callback_data_file).unwrap()),
        )
        .route(
            "/webhook",
            post(github_app::webhook)
                .with_state(GithubApp::<WebhookState>::new(&args.webhook_data_file).unwrap()),
        )
        .layer(middleware::from_fn_with_state(
            logging_state,
            request_logging::log,
        ))
        .route("/login", get(auth::login_page).post(auth::login))
        .route("/logout", get(auth::logout))
        .layer(auth::auth_layer())
        .layer(auth::session_layer());

    let addr = &([0, 0, 0, 0], args.port).into();

    log::debug!("listening on: {:?}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
