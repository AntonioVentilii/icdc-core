use candid::CandidType;
use serde::Deserialize;
use serde_bytes::ByteBuf;

/// Represents a header field in an HTTP request or response.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct HeaderField(pub String, pub String);

/// Represents an incoming HTTP request to the canister.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct HttpRequest {
    /// The URL of the request.
    pub url: String,
    /// The HTTP method (e.g., "GET", "POST").
    pub method: String,
    /// The request body.
    pub body: ByteBuf,
    /// The request headers.
    pub headers: Vec<HeaderField>,
}

/// Represents an HTTP response from the canister.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct HttpResponse {
    /// The response body.
    pub body: ByteBuf,
    /// The response headers.
    pub headers: Vec<HeaderField>,
    /// The HTTP status code.
    pub status_code: u16,
}
