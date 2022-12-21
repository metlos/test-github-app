use auth::User;
use axum::{
    middleware,
    routing::{get, get_service, post},
    Router,
};
use clap::Parser;
use env_logger::Env;
use github_app::{CallbackState, GithubApp};
use hyper::StatusCode;
use tower_http::services::ServeFile;

use crate::github_app::{ApiCallState, WebhookState};

mod auth;
mod github_app;
mod jwt;
mod request_logging;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env, default_value_t = 3000, env)]
    port: u16,

    #[arg(long, env, default_value = "123456")]
    access_password: String,

    #[arg(long, env, default_value = "interactions.log")]
    interactions_file: String,

    #[arg(long, env, default_value = "callback.data")]
    callback_data_file: String,

    #[arg(long, env, default_value = "webhook.data")]
    webhook_data_file: String,

    #[arg(long, env, default_value = "private-key.pem")]
    private_key_file: String,

    #[arg(long, env)]
    app_id: String,

    #[arg(long, env, default_value = "SPI Test GitHub App")]
    app_name: String,
}

async fn could_not_serve_html(_: std::io::Error) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "could not serve static html".to_owned(),
    )
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

    auth::DATABASE
        .write()
        .await
        .insert(allowed_user.login.clone(), allowed_user);

    let client = reqwest::Client::default();

    let jwt = jwt::Jwt::new(
        std::fs::read_to_string(args.private_key_file)
            .unwrap()
            .as_bytes(),
    )
    .unwrap();

    let api_state = ApiCallState {
        client: client.clone(),
        jwt,
        app_id: args.app_id,
        app_name: args.app_name,
    };

    let app = Router::new()
        .route(
            "/",
            get_service(ServeFile::new("html/index.html"))
                .handle_error(could_not_serve_html)
                .layer(middleware::from_fn_with_state(
                    "/login?to=./".to_owned(),
                    auth::redirect_on_no_auth,
                )),
        )
        .route(
            "/callback",
            get(github_app::callback).with_state(
                GithubApp::<CallbackState>::new(&args.callback_data_file, client.clone()).unwrap(),
            ),
        )
        .route(
            "/webhook",
            post(github_app::webhook).with_state(
                GithubApp::<WebhookState>::new(&args.webhook_data_file, client.clone()).unwrap(),
            ),
        )
        .route(
            "/login",
            get_service(ServeFile::new("html/login.html")).handle_error(could_not_serve_html),
        )
        .route("/login", post(auth::login))
        .route("/logout", get(auth::logout))
        .route(
            "/list-installations",
            get(github_app::list_installations)
                .with_state(api_state.clone())
                .layer(middleware::from_fn_with_state(
                    "/login?to=list-installations".to_owned(),
                    auth::redirect_on_no_auth,
                )),
        )
        .route(
            "/installation-access-token",
            get(github_app::installation_access_token).with_state(api_state.clone()),
        )
        .route(
            "/incoming",
            get_service(ServeFile::new(args.interactions_file))
                .handle_error(could_not_serve_html)
                .layer(middleware::from_fn_with_state(
                    "/login?to=incoming".to_owned(),
                    auth::redirect_on_no_auth,
                )),
        )
        .layer(middleware::from_fn_with_state(
            logging_state,
            request_logging::log,
        ))
        .layer(auth::auth_layer())
        .layer(auth::session_layer());

    let addr = &([0, 0, 0, 0], args.port).into();

    log::debug!("listening on: {:?}", addr);

    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
