use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use shared::proto::{azookey_service_client::AzookeyServiceClient, *};
use std::time::Instant;
use tokio::net::windows::named_pipe::ClientOptions;
use tonic::transport::Endpoint;
use tower::service_fn;

// A small regression set, not a claim of general conversion accuracy.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AZOOKEY_INSTANCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err("Set AZOOKEY_INSTANCE to an isolated test instance".into());
    }
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../docs/fixtures/conversion-quality.json"
    ))?;
    let channel = Endpoint::try_from("http://localhost")?
        .connect_with_connector(service_fn(|_| async {
            ClientOptions::new()
                .open(shared::pipe_path("azookey_server"))
                .map(TokioIo::new)
        }))
        .await?;
    let mut client = AzookeyServiceClient::new(channel);
    let mut records = Vec::new();
    let mut timings = Vec::new();
    // Warm the model and dictionary with exactly the same set for each configuration.
    for round in 0..2 {
        for case in &cases {
            client
                .clear_text(ClearTextRequest { preview_only: true })
                .await?;
            let raw = case["raw_input"].as_str().unwrap_or_default();
            let reading = if raw.is_empty() {
                case["reading"]
                    .as_str()
                    .ok_or("Missing reading")?
                    .to_string()
            } else {
                client
                    .append_text(AppendTextRequest {
                        text_to_append: raw.into(),
                        preview_only: true,
                    })
                    .await?
                    .into_inner()
                    .composing_text
                    .ok_or("Missing reading")?
                    .hiragana
            };
            let start = Instant::now();
            let result = client
                .convert_text(ConvertTextRequest {
                    reading,
                    raw_input: raw.into(),
                    context: case["context"].as_str().unwrap_or_default().into(),
                    prediction_only: false,
                })
                .await?
                .into_inner()
                .composing_text
                .ok_or("Missing candidates")?;
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert!(result.suggestions.iter().all(|s| !s.is_prediction));
            if round == 0 {
                continue;
            }
            let outputs: Vec<String> = result
                .suggestions
                .iter()
                .map(|s| format!("{}{}", s.text, s.subtext))
                .collect();
            let answers = case["answers"].as_array().ok_or("Missing answers")?;
            let rank = outputs
                .iter()
                .position(|output| answers.iter().any(|a| a.as_str() == Some(output.as_str())))
                .map(|i| i + 1);
            timings.push(ms);
            records.push(json!({"id":case["id"],"category":case["category"],"answers":answers,"outputs":outputs,"rank":rank,"latency_ms":ms}));
        }
    }
    timings.sort_by(f64::total_cmp);
    let categories: Vec<_> = ["daily", "work", "names", "english", "context"]
        .iter()
        .map(|category| {
            let items: Vec<_> = records
                .iter()
                .filter(|r| r["category"].as_str() == Some(category))
                .collect();
            json!({"category":category,"count":items.len(),
            "top1":items.iter().filter(|r| r["rank"].as_u64() == Some(1)).count(),
            "top5":items.iter().filter(|r| r["rank"].as_u64().is_some_and(|rank| rank<=5)).count(),
            "in_candidates":items.iter().filter(|r| r["rank"].as_u64().is_some()).count()})
        })
        .collect();
    println!(
        "{}",
        json!({"cases":records.len(),"median_ms":timings[timings.len()/2],"p95_ms":timings[(timings.len()-1)*95/100],"categories":categories,"records":records})
    );
    Ok(())
}
