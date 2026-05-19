use azookey_converter::{Candidate as NativeCandidate, NativeConverter};
use azookey_server::TonicNamedPipeServer;
use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};
use tonic_reflection::server::Builder as ReflectionBuilder;

use shared::proto::azookey_service_server::{AzookeyService, AzookeyServiceServer};
use shared::proto::{
    AppendTextRequest, AppendTextResponse, ClearTextRequest, ClearTextResponse, ComposingText,
    MoveCursorRequest, MoveCursorResponse, RemoveTextRequest, RemoveTextResponse,
    ShrinkTextRequest, ShrinkTextResponse, Suggestion,
};

fn response_from_parts(hiragana: String, candidates: Vec<NativeCandidate>) -> ComposingText {
    let suggestions = candidates
        .into_iter()
        .map(|candidate| Suggestion {
            text: candidate.text,
            subtext: candidate.subtext,
            corresponding_count: candidate.corresponding_count,
        })
        .collect();

    ComposingText {
        hiragana,
        suggestions,
    }
}

#[derive(Debug, Clone)]
pub struct MyAzookeyService {
    converter: Arc<Mutex<NativeConverter>>,
}

impl MyAzookeyService {
    fn new(converter: NativeConverter) -> Self {
        Self {
            converter: Arc::new(Mutex::new(converter)),
        }
    }

    fn with_converter<T>(
        &self,
        update: impl FnOnce(&mut NativeConverter) -> T,
    ) -> Result<T, Status> {
        let mut converter = self
            .converter
            .lock()
            .map_err(|_| Status::internal("converter mutex poisoned"))?;
        Ok(update(&mut converter))
    }
}

#[tonic::async_trait]
impl AzookeyService for MyAzookeyService {
    async fn append_text(
        &self,
        request: Request<AppendTextRequest>,
    ) -> Result<Response<AppendTextResponse>, Status> {
        let input = request.into_inner().text_to_append;
        let composing_text = self.with_converter(|converter| {
            let candidates = converter.append_text(&input);
            response_from_parts(converter.hiragana().to_string(), candidates)
        })?;

        Ok(Response::new(AppendTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn remove_text(
        &self,
        _: Request<RemoveTextRequest>,
    ) -> Result<Response<RemoveTextResponse>, Status> {
        let composing_text = self.with_converter(|converter| {
            let candidates = converter.remove_text();
            response_from_parts(converter.hiragana().to_string(), candidates)
        })?;

        Ok(Response::new(RemoveTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn move_cursor(
        &self,
        _: Request<MoveCursorRequest>,
    ) -> Result<Response<MoveCursorResponse>, Status> {
        let composing_text = self.with_converter(|converter| {
            response_from_parts(converter.hiragana().to_string(), converter.candidates())
        })?;

        Ok(Response::new(MoveCursorResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn clear_text(
        &self,
        _: Request<ClearTextRequest>,
    ) -> Result<Response<ClearTextResponse>, Status> {
        self.with_converter(|converter| converter.clear_text())?;
        Ok(Response::new(ClearTextResponse {}))
    }

    async fn shrink_text(
        &self,
        request: Request<ShrinkTextRequest>,
    ) -> Result<Response<ShrinkTextResponse>, Status> {
        let offset = request.into_inner().offset;
        let composing_text = self.with_converter(|converter| {
            let candidates = converter.shrink_text(offset);
            response_from_parts(converter.hiragana().to_string(), candidates)
        })?;

        Ok(Response::new(ShrinkTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn set_context(
        &self,
        request: Request<shared::proto::SetContextRequest>,
    ) -> Result<Response<shared::proto::SetContextResponse>, Status> {
        let context = request.into_inner().context;
        let trimmed_context = context
            .split('\r')
            .filter(|s| !s.is_empty())
            .next_back()
            .unwrap_or_default()
            .to_string();

        self.with_converter(|converter| converter.set_context(trimmed_context))?;
        Ok(Response::new(shared::proto::SetContextResponse {}))
    }

    async fn update_config(
        &self,
        _: Request<shared::proto::UpdateConfigRequest>,
    ) -> Result<Response<shared::proto::UpdateConfigResponse>, Status> {
        Ok(Response::new(shared::proto::UpdateConfigResponse {}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("AzookeyServer started");
    let current_exe = std::env::current_exe()?;
    let resource_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let converter = NativeConverter::load(resource_dir);
    let service = MyAzookeyService::new(converter);

    println!("AzookeyServer listening");

    Server::builder()
        .add_service(AzookeyServiceServer::new(service))
        .add_service(
            ReflectionBuilder::configure()
                .register_encoded_file_descriptor_set(shared::proto::FILE_DESCRIPTOR_SET)
                .build_v1()
                .unwrap(),
        )
        .serve_with_incoming(TonicNamedPipeServer::new("azookey_server"))
        .await?;

    Ok(())
}
