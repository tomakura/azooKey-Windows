use azookey_server::TonicNamedPipeServer;
use tonic::{transport::Server, Request, Response, Status};
use tonic_reflection::server::Builder as ReflectionBuilder;

use shared::proto::azookey_service_server::{AzookeyService, AzookeyServiceServer};
use shared::proto::{
    AppendTextRequest, AppendTextResponse, ClearTextRequest, ClearTextResponse,
    CommitCandidateRequest, CommitCandidateResponse, ComposingText, MoveCursorRequest,
    MoveCursorResponse, RemoveTextRequest, RemoveTextResponse, ShrinkTextRequest,
    ShrinkTextResponse, Suggestion,
};

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::Mutex;

const USE_ZENZAI: bool = true;

#[derive(Debug, Clone)]
#[repr(C)]
struct FFICandidate {
    text: *mut c_char,
    subtext: *mut c_char,
    hiragana: *mut c_char,
    corresponding_count: c_int,
}

unsafe extern "C" {
    fn Initialize(path: *const c_char, use_zenzai: bool);
    fn SetContext(context: *const c_char);
    fn AppendText(input: *const c_char, cursorPtr: *mut c_int) -> *mut c_char;
    fn RemoveText(cursorPtr: *mut c_int) -> *mut c_char;
    fn MoveCursor(offset: c_int, cursorPtr: *mut c_int) -> *mut c_char;
    fn ShrinkText(offset: c_int) -> *mut c_char;
    fn ClearText();
    fn GetComposedText(lengthPtr: *mut c_int) -> *mut *mut FFICandidate;
    fn LoadConfig();
}

// The Swift engine keeps global state, so serialize all FFI access.
static ENGINE_LOCK: Mutex<()> = Mutex::new(());

fn with_engine<T>(action: impl FnOnce() -> T) -> T {
    let _guard = ENGINE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    action()
}

fn initialize(path: &str) {
    let path = CString::new(path).expect("CString::new failed");
    with_engine(|| unsafe { Initialize(path.as_ptr(), USE_ZENZAI) });
}

fn add_text(input: &str) -> String {
    let input = CString::new(input).unwrap_or_default();
    with_engine(|| unsafe {
        let mut cursor: c_int = 0;
        let result = AppendText(input.as_ptr(), &mut cursor);
        CStr::from_ptr(result).to_string_lossy().into_owned()
    })
}

fn move_cursor(offset: i32) -> String {
    with_engine(|| unsafe {
        let mut cursor: c_int = 0;
        let result = MoveCursor(offset, &mut cursor);
        CStr::from_ptr(result).to_string_lossy().into_owned()
    })
}

fn remove_text() -> String {
    with_engine(|| unsafe {
        let mut cursor: c_int = 0;
        let result = RemoveText(&mut cursor);
        CStr::from_ptr(result).to_string_lossy().into_owned()
    })
}

fn clear_text() {
    with_engine(|| unsafe { ClearText() });
}

fn shrink_text(offset: i32) -> String {
    with_engine(|| unsafe {
        let result = ShrinkText(offset);
        CStr::from_ptr(result).to_string_lossy().into_owned()
    })
}

fn get_composed_text() -> Vec<Suggestion> {
    with_engine(|| unsafe {
        let mut length: c_int = 0;
        let result = GetComposedText(&mut length);
        let mut suggestions: Vec<Suggestion> = Vec::with_capacity(length as usize);

        for index in 0..length as usize {
            let candidate = (**result.add(index)).clone();
            let text = CStr::from_ptr(candidate.text)
                .to_string_lossy()
                .into_owned();
            let subtext = CStr::from_ptr(candidate.subtext)
                .to_string_lossy()
                .into_owned();
            let corresponding_count = candidate.corresponding_count;

            if suggestions.iter().any(|s| s.text == text) {
                continue;
            }
            suggestions.push(Suggestion {
                text,
                subtext,
                corresponding_count,
            });
        }

        suggestions
    })
}

fn composing_text_response(hiragana: String) -> ComposingText {
    ComposingText {
        hiragana,
        suggestions: get_composed_text(),
    }
}

#[derive(Debug, Default)]
pub struct MyAzookeyService;

#[tonic::async_trait]
impl AzookeyService for MyAzookeyService {
    async fn append_text(
        &self,
        request: Request<AppendTextRequest>,
    ) -> Result<Response<AppendTextResponse>, Status> {
        let input = request.into_inner().text_to_append;
        let hiragana = add_text(&input);

        Ok(Response::new(AppendTextResponse {
            composing_text: Some(composing_text_response(hiragana)),
        }))
    }

    async fn remove_text(
        &self,
        _: Request<RemoveTextRequest>,
    ) -> Result<Response<RemoveTextResponse>, Status> {
        let hiragana = remove_text();

        Ok(Response::new(RemoveTextResponse {
            composing_text: Some(composing_text_response(hiragana)),
        }))
    }

    async fn move_cursor(
        &self,
        request: Request<MoveCursorRequest>,
    ) -> Result<Response<MoveCursorResponse>, Status> {
        let offset = request.into_inner().offset;
        let hiragana = move_cursor(offset);

        Ok(Response::new(MoveCursorResponse {
            composing_text: Some(composing_text_response(hiragana)),
        }))
    }

    async fn clear_text(
        &self,
        _: Request<ClearTextRequest>,
    ) -> Result<Response<ClearTextResponse>, Status> {
        clear_text();
        Ok(Response::new(ClearTextResponse {}))
    }

    async fn shrink_text(
        &self,
        request: Request<ShrinkTextRequest>,
    ) -> Result<Response<ShrinkTextResponse>, Status> {
        let offset = request.into_inner().offset;
        let hiragana = shrink_text(offset);

        Ok(Response::new(ShrinkTextResponse {
            composing_text: Some(composing_text_response(hiragana)),
        }))
    }

    async fn set_context(
        &self,
        request: Request<shared::proto::SetContextRequest>,
    ) -> Result<Response<shared::proto::SetContextResponse>, Status> {
        let context = request.into_inner().context;
        let trimmed_context = context
            .split('\r')
            .rfind(|s| !s.is_empty())
            .unwrap_or_default()
            .to_string();

        let context = CString::new(trimmed_context).unwrap_or_default();
        with_engine(|| unsafe { SetContext(context.as_ptr()) });
        Ok(Response::new(shared::proto::SetContextResponse {}))
    }

    async fn update_config(
        &self,
        _: Request<shared::proto::UpdateConfigRequest>,
    ) -> Result<Response<shared::proto::UpdateConfigResponse>, Status> {
        with_engine(|| unsafe { LoadConfig() });
        Ok(Response::new(shared::proto::UpdateConfigResponse {}))
    }

    async fn commit_candidate(
        &self,
        _: Request<CommitCandidateRequest>,
    ) -> Result<Response<CommitCandidateResponse>, Status> {
        // The Swift converter does not expose learning yet; accept and ignore.
        Ok(Response::new(CommitCandidateResponse {}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("AzookeyServer started");
    let current_exe = std::env::current_exe()?;
    let parent_dir = current_exe
        .parent()
        .ok_or("Failed to resolve server directory")?;
    initialize(&parent_dir.to_string_lossy());

    let service = MyAzookeyService;

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
