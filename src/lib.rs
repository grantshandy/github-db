mod error;

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
pub use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

pub use error::ClientError;

/// The entrypoint for your database connection.
#[derive(Clone, Debug)]
pub struct Client {
    owner: String,
    repo: String,
    host: Url,
    path_prefix: Option<String>,
    client: reqwest::Client,
}

impl Client {
    /// Create a new [`Client`].
    pub fn new(
        auth_token: impl AsRef<str>,
        owner: impl AsRef<str>,
        repo: impl AsRef<str>,
        host: Option<String>,
        path_prefix: Option<String>,
    ) -> Result<Self, ClientError> {
        let auth = auth_token.as_ref().to_string();
        let owner = owner.as_ref().to_string();
        let repo = repo.as_ref().to_string();

        let host: Url = match host {
            Some(host) => match Url::parse(&host) {
                Ok(host) => host,
                Err(err) => return Err(ClientError::Parse(err)),
            },
            None => match Url::parse("https://api.github.com") {
                Ok(host) => host,
                Err(err) => return Err(ClientError::Parse(err)),
            },
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            HeaderValue::from_str("application/vnd.github+json").unwrap(),
        );
        headers.insert(
            "Authorization",
            HeaderValue::from_str(format!("Bearer {auth}").as_str()).unwrap(),
        );

        let builder = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(&format!("{}-{}", &owner, &repo));

        let client = match builder.build() {
            Ok(client) => client,
            Err(err) => return Err(ClientError::Http(err)),
        };

        Ok(Self {
            owner,
            repo,
            host,
            path_prefix,
            client,
        })
    }

    fn create_url(&self, path: Option<&str>) -> Url {
        let mut base_url = self.host.clone();

        let prefix = &self.path_prefix.clone().unwrap_or_default();

        base_url.set_path(&format!(
            "/repos/{}/{}/contents/{}{}",
            self.owner,
            self.repo,
            prefix,
            path.unwrap_or_default(),
        ));

        base_url
    }

    /// Return a reference to a collection in the database.
    ///
    /// If it doesn't exist in the repository it'll be created automatically
    pub async fn collection<T: Serialize + DeserializeOwned>(
        &self,
        name: impl AsRef<str>,
    ) -> Result<Collection<T>, ClientError> {
        let name = name.as_ref().to_string();
        let url = self.create_url(Some(&format!("{name}.json"))).clone();

        // start by trying to get the document to see if it's already there
        let get_bytes: Option<Bytes> = match self.client.get(url.clone()).send().await {
            Ok(response) => {
                if response.status() == 404 {
                    None
                } else {
                    match response.bytes().await {
                        Ok(bytes) => Some(bytes),
                        Err(e) => return Err(ClientError::Http(e)),
                    }
                }
            }
            Err(e) => return Err(ClientError::Http(e)),
        };

        // if there was a 404 for trying to get it then we try to create an empty document
        let bytes: Bytes = match get_bytes {
            Some(b) => b,
            None => {
                let request_body = format!(
                    "{{\"message\":\"Creating Collection '{}'\",\"content\":\"{}\"}}",
                    &name,
                    base64::encode("[]".as_bytes())
                );

                match self.client.put(url.clone()).body(request_body).send().await {
                    Ok(response) => match response.bytes().await {
                        Ok(r) => r,
                        Err(e) => return Err(ClientError::Http(e)),
                    },
                    Err(e) => return Err(ClientError::Http(e)),
                }
            }
        };

        let json: Value = match serde_json::from_slice(&bytes) {
            Ok(json) => json,
            Err(err) => return Err(ClientError::Json(err)),
        };

        let inner: Vec<T> = if let Some(content_value) = json.get("content") {
            decode_serde_base64(content_value)?
        } else {
            return Err(ClientError::NoContent);
        };

        // github requires we send along a sha with our updates so we store it every time we download
        let sha = if let Some(sha) = json.get("sha") {
            sha.to_string().replace('"', "")
        } else {
            return Err(ClientError::NoSha);
        };

        Ok(Collection {
            name,
            url,
            client: self.client.clone(),
            inner,
            sha,
        })
    }
}

/// A collection of documents in the database
pub struct Collection<T> {
    pub name: String,
    url: Url,
    client: reqwest::Client,
    sha: String,
    inner: Vec<T>,
}

impl<T: Serialize + DeserializeOwned> Collection<T> {
    /// update client state to be in line with the database
    pub async fn update(&mut self) -> Result<(), ClientError> {
        let bytes: Bytes = match self.client.get(self.url.clone()).send().await {
            Ok(response) => match response.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => return Err(ClientError::Http(e)),
            },
            Err(e) => return Err(ClientError::Http(e)),
        };

        let json: Value = match serde_json::from_slice(&bytes) {
            Ok(json) => json,
            Err(err) => return Err(ClientError::Json(err)),
        };

        self.inner = if let Some(content_value) = json.get("content") {
            decode_serde_base64(content_value)?
        } else {
            return Err(ClientError::NoContent);
        };

        // github requires we send along a sha with our updates so we store it every time we download
        self.sha = if let Some(sha) = json.get("sha") {
            sha.to_string().replace('"', "")
        } else {
            return Err(ClientError::NoSha);
        };

        Ok(())
    }

    /// push document to the database
    pub async fn insert(&mut self, data: T) -> Result<(), ClientError> {
        self.update().await?;

        self.inner.push(data);

        let inner_json = match serde_json::to_string(&self.inner) {
            Ok(json) => json,
            Err(err) => return Err(ClientError::Json(err)),
        };

        let request_body = match serde_json::to_string(&json!({
            "message": "Insert",
            "content": base64::encode(inner_json.as_bytes()),
            "sha": self.sha,
        })) {
            Ok(body) => body,
            Err(err) => return Err(ClientError::Json(err)),
        };

        let _response: Value = match self
            .client
            .put(self.url.clone())
            .body(request_body)
            .send()
            .await
        {
            Ok(response) => match response.json().await {
                Ok(r) => r,
                Err(e) => return Err(ClientError::Http(e)),
            },
            Err(e) => return Err(ClientError::Http(e)),
        };

        Ok(())
    }

    /// overwrite the entire collection
    pub async fn set_as(&mut self, value: Vec<T>) -> Result<(), ClientError> {
        self.update().await?;

        self.inner = value;

        let inner_json = match serde_json::to_string(&self.inner) {
            Ok(json) => json,
            Err(err) => return Err(ClientError::Json(err)),
        };

        let request_body = match serde_json::to_string(&json!({
            "message": "Overwrite",
            "content": base64::encode(inner_json.as_bytes()),
            "sha": self.sha,
        })) {
            Ok(body) => body,
            Err(err) => return Err(ClientError::Json(err)),
        };

        let _response: Value = match self
            .client
            .put(self.url.clone())
            .body(request_body)
            .send()
            .await
        {
            Ok(response) => match response.json().await {
                Ok(r) => r,
                Err(e) => return Err(ClientError::Http(e)),
            },
            Err(e) => return Err(ClientError::Http(e)),
        };

        Ok(())
    }

    /// syncs and returns all documents
    pub async fn data(&mut self) -> Result<&Vec<T>, ClientError> {
        self.update().await?;

        Ok(&self.inner)
    }
}

fn decode_serde_base64<T: DeserializeOwned>(value: &Value) -> Result<Vec<T>, ClientError> {
    // both serde_json and github are messing me up here.
    // it puts "\n" into the base64? I have no idea why.
    let content_encoded = value.to_string().replace("\\n", "");
    let content_encoded = content_encoded
        .split_at(content_encoded.len() - 1)
        .0
        .split_at(1)
        .1;

    let content_decoded = match base64::decode(content_encoded) {
        Ok(decoded) => decoded,
        Err(err) => return Err(ClientError::BadEncoding(err)),
    };

    let data: Vec<T> = match serde_json::from_slice(&content_decoded) {
        Ok(inner) => inner,
        Err(err) => return Err(ClientError::Json(err)),
    };

    Ok(data)
}
