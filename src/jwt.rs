use std::{
    fmt::Display,
    time::{SystemTime, SystemTimeError},
};

use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Jwt {
    key: EncodingKey,
}

impl Jwt {
    pub fn new(pem_data: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            key: EncodingKey::from_rsa_pem(pem_data)?,
        })
    }

    pub fn for_app<'a, 's: 'a>(&'a self, app_id: &'s str) -> AppToken<'a> {
        AppToken {
            app_id,
            key: &self.key,
        }
    }
}

#[derive(Clone)]
pub struct AppToken<'a> {
    app_id: &'a str,
    key: &'a EncodingKey,
}

pub struct InstallationToken<'a> {
    installation_id: &'a str,
    app_token: &'a AppToken<'a>,
}

impl<'a> AppToken<'a> {
    pub fn generate(&self) -> Result<String, Error> {
        let now = SystemTime::now();
        let minute_ago = now.duration_since(SystemTime::UNIX_EPOCH)?.as_secs() - 60;
        let in_ten_minutes = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + (10 * 60);

        Ok(jsonwebtoken::encode(
            &Header::new(jsonwebtoken::Algorithm::RS256),
            &Claim {
                iat: minute_ago,
                exp: in_ten_minutes,
                iss: self.app_id.to_owned(),
            },
            &self.key,
        )?)
    }

    
}

#[derive(Debug, Serialize, Deserialize)]
struct Claim {
    iat: u64,
    exp: u64,
    iss: String,
}

#[derive(Debug, Clone)]
pub enum Error {
    Time(SystemTimeError),
    Jwt(jsonwebtoken::errors::Error),
}

impl From<SystemTimeError> for Error {
    fn from(e: SystemTimeError) -> Self {
        Error::Time(e)
    }
}

impl From<jsonwebtoken::errors::Error> for Error {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        Error::Jwt(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Time(e) => e.fmt(f),
            Error::Jwt(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}
