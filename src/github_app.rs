use std::{collections::HashSet, error::Error};

use axum::extract::Query;
use axum::{extract::State, response::IntoResponse, Json};
use hyper::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct GithubApp<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    file_name: String,
    data: T,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CallbackState {
    callback_code: String,
    installation_ids: HashSet<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WebhookState {}

impl<T> GithubApp<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub fn new(file_name: &str) -> Result<Self, Box<dyn Error>> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(file_name)?;

        let data = serde_json::from_reader(f).unwrap_or(Default::default());
        Ok(Self {
            file_name: file_name.to_owned(),
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
