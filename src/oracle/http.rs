use std::fmt::Debug;

use ic_cdk::management_canister::{
    http_request, HttpHeader, HttpMethod, HttpRequestArgs, TransformContext,
};

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum HttpError {
    #[error("Could not parse response body")]
    CouldNotParseResponseBody,
    #[error("Not found")]
    NotFound,
    #[error("{0}")]
    Generic(String),
}

pub async fn http_get_call(
    url: String,
    headers: Vec<HttpHeader>,
    transform: Option<TransformContext>,
) -> Result<Value, HttpError> {
    //note "CanisterHttpRequestArgument" and "HttpMethod" are declared in line 4
    let request = HttpRequestArgs {
        url: url.clone(),
        method: HttpMethod::GET,
        body: None,               //optional for request
        max_response_bytes: Some(100 * 1024), //100 kb max response size
        transform,                //optional for request
        headers,
    };

    match http_request(&request).await {
        //See:https://docs.rs/ic-cdk/latest/ic_cdk/api/management_canister/http_request/struct.HttpResponse.html
        Ok(response) => {
            if response.status == 404u32 {
                return Err(HttpError::NotFound);
            }

            let str_body = String::from_utf8(response.body)
                .expect("Transformed response is not UTF-8 encoded.");


            serde_json::from_str(&str_body).or(Err(HttpError::CouldNotParseResponseBody))
        }
        Err(e) => {
            let message = format!(
                "The http_request resulted into error. Error: {}",
                e.to_string()
            );
            //Return the error as a string and end the method
            Err(HttpError::Generic(message))
        }
    }
}
