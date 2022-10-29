use std::{
    error::Error,
    fmt::{self, Display},
};

use url::ParseError;

/// Any error this library can return.
#[derive(Debug)]
pub enum ClientError {
    Parse(ParseError),
    Http(reqwest::Error),
    Json(serde_json::Error),
    NoContent,
    BadEncoding(base64::DecodeError),
    NotUtf8,
    NoSha,
}

impl Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Parse(e) => write!(f, "Parse Error: {e}"),
            ClientError::Http(e) => write!(f, "Http Error: {e}"),
            ClientError::Json(e) => write!(f, "Json Parsing Error: {e}"),
            ClientError::NoContent => write!(f, "No Content Found In Returned JSON"),
            ClientError::BadEncoding(e) => write!(f, "Base64 Decode Error: {e}"),
            ClientError::NotUtf8 => write!(f, "Content Not Encoded in Utf8"),
            ClientError::NoSha => write!(f, "No Sha Returned From Github"),
        }
    }
}

impl Error for ClientError {}
