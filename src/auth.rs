use std::{collections::HashMap, sync::Arc};

use axum::{
    body::boxed,
    extract::{Form, State},
    headers::Header,
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use axum_login::{
    axum_sessions::SessionLayer, memory_store::MemoryStore as AuthMemoryStore, secrecy::SecretVec,
    AuthLayer, AuthUser,
};
use axum_sessions::async_session::MemoryStore as SessionMemoryStore;
use hyper::{Body, Request, StatusCode};
use lazy_static::lazy_static;
use rand::Rng;
use serde::Deserialize;
use tokio::sync::RwLock;

lazy_static! {
    pub static ref DATABASE: Arc<RwLock<HashMap<String, User>>> =
        Arc::new(RwLock::new(HashMap::new()));
    static ref SECRET: [u8; 64] = rand::thread_rng().gen::<[u8; 64]>();
}

#[derive(Debug, Clone)]
pub struct User {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub login: String,
    pub password: String,
}

impl AuthUser for User {
    fn get_id(&self) -> String {
        self.login.clone()
    }

    fn get_password_hash(&self) -> axum_login::secrecy::SecretVec<u8> {
        SecretVec::new(self.password.clone().into())
    }
}

type AuthContext = axum_login::extractors::AuthContext<User, AuthMemoryStore<User>>;

pub fn session_layer() -> SessionLayer<SessionMemoryStore> {
    let store = SessionMemoryStore::new();

    SessionLayer::new(store, SECRET.as_ref())
}

pub fn auth_layer() -> AuthLayer<AuthMemoryStore<User>, User> {
    let store = AuthMemoryStore::new(&DATABASE);
    AuthLayer::new(store, SECRET.as_ref())
}

pub async fn login(mut auth: AuthContext, Form(form): Form<LoginForm>) -> impl IntoResponse {
    if let Some(user) = DATABASE.read().await.get(&form.login) {
        if user.password == form.password {
            return match auth.login(&user).await {
                Ok(_) => (StatusCode::OK, "logged in".to_owned()),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to login: {}", e),
                ),
            };
        }
    }

    (StatusCode::FORBIDDEN, "invalid creds".to_owned())
}

pub async fn logout(mut auth: AuthContext) {
    auth.logout().await;
}

pub async fn redirect_on_no_auth<B>(
    State(redirect_path): State<String>,
    auth: AuthContext,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    if auth.current_user.is_none() {
        Response::builder()
            .status(axum::http::StatusCode::TEMPORARY_REDIRECT)
            .header(axum::headers::Location::name(), redirect_path)
            .body(boxed(Body::empty()))
            .unwrap()
    } else {
        next.run(req).await
    }
}

pub async fn login_page() -> impl IntoResponse {
    Html(
        r#"
<html>
    <body>
        <form method=post>
        login:<input name="login"/><br/>
        password:<input name="password" type="password"/><br/>
        <input type="submit"/>
        </form>
    </body>
</html>    
    "#,
    )
}
