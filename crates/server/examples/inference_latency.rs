use hyper_util::rt::TokioIo;
use shared::proto::{azookey_service_client::AzookeyServiceClient, *};
use std::time::Instant;
use tokio::net::windows::named_pipe::ClientOptions;
use tonic::transport::Endpoint;
use tower::service_fn;

// Compare the same model/settings through the actual Space-conversion RPC.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AZOOKEY_INSTANCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err("Set AZOOKEY_INSTANCE to an isolated test instance".into());
    }
    let channel = Endpoint::try_from("http://localhost")?
        .connect_with_connector(service_fn(|_| async {
            ClientOptions::new()
                .open(shared::pipe_path("azookey_server"))
                .map(TokioIo::new)
        }))
        .await?;
    let mut client = AzookeyServiceClient::new(channel);
    let inputs = [
        "かんじ",
        "きょうはいいてんきですね",
        "にほんごのぶんしょうをにゅうりょくしてへんかんのそくどをかくにんします",
        "らいしゅうのかいぎではあたらしいぷろじぇくとのすすみぐあいについてほうこくするよていです",
    ];
    let mut samples = vec![Vec::new(); inputs.len()];
    let mut candidates = vec![Vec::new(); inputs.len()];
    let mut cold_ms = 0.0;
    for round in 0..8 {
        for (index, reading) in inputs.iter().enumerate() {
            client
                .clear_text(ClearTextRequest { preview_only: true })
                .await?;
            let start = Instant::now();
            let result = client
                .convert_text(ConvertTextRequest {
                    reading: (*reading).into(),
                    raw_input: String::new(),
                    context: "日本語入力の動作を確認しています。".into(),
                    prediction_only: false,
                })
                .await?
                .into_inner()
                .composing_text
                .ok_or("Missing composing text")?;
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert!(!result.suggestions.is_empty());
            assert!(result.suggestions.iter().all(|s| !s.is_prediction));
            if index == 0 {
                assert!(result.suggestions.iter().any(|s| s.text == "漢字"));
            }
            if round == 0 && index == 0 {
                cold_ms = ms;
            }
            if round > 0 {
                samples[index].push(ms);
            }
            candidates[index] = result.suggestions.iter().map(|s| s.text.clone()).collect();
        }
    }
    let results: Vec<_> = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            samples[index].sort_by(f64::total_cmp);
            let timings = &samples[index];
            serde_json::json!({
                "reading": input, "characters": input.chars().count(), "samples": timings.len(),
                "median_ms": timings[timings.len()/2], "max_ms": timings.last(),
                "candidates": candidates[index],
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"cold_first_conversion_ms": cold_ms, "results": results})
    );
    Ok(())
}
