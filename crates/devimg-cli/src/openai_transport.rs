use std::path::Path;

use devimg_core::{
    AiAltProviderPayload, AiAltRequest, AiDraftProviderPayload, AiDraftRequest,
    AiReviewProviderPayload, AiReviewRequest,
};
use serde_json::Value;

use super::{
    call_openai_alt_with_transport, call_openai_draft_with_transport,
    call_openai_review_with_transport, CliError, OpenAiResponsesTransport, OpenAiTransportError,
};

struct ReqwestOpenAiResponsesTransport;

impl OpenAiResponsesTransport for ReqwestOpenAiResponsesTransport {
    fn post_responses(
        &self,
        api_key: &str,
        body: &Value,
    ) -> std::result::Result<Value, OpenAiTransportError> {
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|_| OpenAiTransportError::BuildClient)?;
        let response = client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(api_key)
            .json(body)
            .send()
            .map_err(|_| OpenAiTransportError::Request)?;
        let status = response.status();
        if !status.is_success() {
            return Err(OpenAiTransportError::Status(status.as_u16()));
        }
        response
            .json()
            .map_err(|_| OpenAiTransportError::InvalidJson)
    }
}

pub(super) fn call_openai_review(
    request: &AiReviewRequest,
    project_root: &Path,
    api_key: &str,
) -> Result<AiReviewProviderPayload, CliError> {
    call_openai_review_with_transport(
        request,
        project_root,
        api_key,
        &ReqwestOpenAiResponsesTransport,
    )
}

pub(super) fn call_openai_alt(
    request: &AiAltRequest,
    project_root: &Path,
    api_key: &str,
) -> Result<AiAltProviderPayload, CliError> {
    call_openai_alt_with_transport(
        request,
        project_root,
        api_key,
        &ReqwestOpenAiResponsesTransport,
    )
}

pub(super) fn call_openai_draft(
    request: &AiDraftRequest,
    api_key: &str,
) -> Result<AiDraftProviderPayload, CliError> {
    call_openai_draft_with_transport(request, api_key, &ReqwestOpenAiResponsesTransport)
}
