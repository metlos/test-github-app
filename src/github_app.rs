use std::{collections::HashSet, error::Error};

use axum::extract::{Query, RawQuery};
use axum::{extract::State, response::IntoResponse, Json};
use hyper::{header, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::jwt;

#[derive(Debug, Clone)]
pub struct GithubApp<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    file_name: String,
    client: reqwest::Client,
    data: T,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CallbackState {
    callback_code: String,
    installation_ids: HashSet<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WebhookState {}

#[derive(Clone)]
pub struct ApiCallState {
    pub client: reqwest::Client,
    pub jwt: jwt::Jwt,
    pub app_id: String,
    pub app_name: String,
}

impl<T> GithubApp<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub fn new(file_name: &str, client: reqwest::Client) -> Result<Self, Box<dyn Error>> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(file_name)?;

        let data = serde_json::from_reader(f).unwrap_or(Default::default());
        Ok(Self {
            file_name: file_name.to_owned(),
            client,
            data,
        })
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let f = std::fs::File::create(&self.file_name)?;

        Ok(serde_json::to_writer(f, &self.data)?)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    code: String,
    installation_id: String,
}

pub async fn callback(
    State(mut state): State<GithubApp<CallbackState>>,
    Query(q): Query<CallbackParams>,
) -> impl IntoResponse {
    state.data.callback_code = q.code;
    state.data.installation_ids.insert(q.installation_id);

    match state.save() {
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to save the state: {}", e),
            )
        }
        _ => {}
    }

    state.client;

    (StatusCode::OK, "processed".to_owned())
}

#[derive(Debug, Deserialize)]
pub struct WebhookPayload {}

pub async fn webhook(
    State(state): State<GithubApp<WebhookState>>,
    Json(_body): Json<WebhookPayload>,
) -> impl IntoResponse {
    // TODO implement more
    match state.save() {
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to save the state: {}", e),
            )
        }
        _ => {}
    }

    (StatusCode::OK, "processed".to_owned())
}

pub async fn list_installations(State(state): State<ApiCallState>) -> impl IntoResponse {
    github_call(
        state,
        reqwest::Method::GET,
        "https://api.github.com/app/installations",
    )
    .await
}

pub async fn installation_access_token(
    State(state): State<ApiCallState>,
    RawQuery(installation_id): RawQuery,
) -> impl IntoResponse {
    match installation_id {
        Some(installation_id) => {
            github_call(
                state,
                reqwest::Method::POST,
                &format!(
                    "https://api.github.com/app/installations/{}/access_tokens",
                    installation_id
                ),
            )
            .await
        }
        None => (
            StatusCode::BAD_REQUEST,
            format!("installation id required as query parameter"),
        ),
    }
}

async fn github_call(
    state: ApiCallState,
    method: reqwest::Method,
    url: &str,
) -> (StatusCode, String) {
    let token = match state.jwt.for_app(&state.app_id).generate() {
        Ok(token) => token,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed produce the app JWT token: {}", e),
            )
        }
    };

    let req = match state
        .client
        .request(method, url)
        .bearer_auth(token)
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::USER_AGENT, state.app_name)
        .build()
    {
        Ok(req) => req,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed build a request to github: {}", e),
            )
        }
    };

    let resp = match state.client.execute(req).await {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed retrieve a response from github: {}", e),
            )
        }
    };

    let body = match resp.text().await {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(body) => body,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "failed to deserialize the response from github into json: {}",
                        e
                    ),
                )
            }
        },
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed read the response body from github: {}", e),
            )
        }
    };

    (StatusCode::OK, serde_json::to_string_pretty(&body).unwrap())
}
