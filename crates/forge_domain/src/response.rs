use anyhow::Result;
use bytes::Bytes;
use derive_setters::Setters;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;

#[derive(Clone, Debug, Default, Setters)]
pub struct Response<Body> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Body,
}

impl Response<Bytes> {
    pub async fn from_reqwest(resp: reqwest::Response) -> Result<Self> {
        let status = resp.status();
        let headers = resp.headers().to_owned();
        let body = resp.bytes().await?;
        Ok(Response { status, headers, body })
    }
}
