use hyper_util::rt::TokioIo;
use shared::proto::{azookey_service_client::AzookeyServiceClient, *};
use std::time::Instant;
use tokio::net::windows::named_pipe::ClientOptions;
use tonic::transport::Endpoint;
use tower::service_fn;

// Use a separate server/profile so verification never changes the user's composition.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AZOOKEY_INSTANCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err("Set AZOOKEY_INSTANCE to an isolated test instance".into());
    }
    let legacy = std::env::args().any(|arg| arg == "--legacy");
    let channel = Endpoint::try_from("http://localhost")?
        .connect_with_connector(service_fn(|_| async {
            ClientOptions::new()
                .open(shared::pipe_path("azookey_server"))
                .map(TokioIo::new)
        }))
        .await?;
    let mut client = AzookeyServiceClient::new(channel);
    client
        .clear_text(ClearTextRequest {
            preview_only: !legacy,
        })
        .await?;
    let input = "kyouhaiitenkidesunihongonyuuryokuwotesutoshiteimasu".repeat(6);
    let limit = if legacy { 60 } else { 240 };
    let mut timings = Vec::new();
    let mut predictions = Vec::new();
    let mut batches = Vec::new();
    let mut reading = String::new();
    for (index, ch) in input.chars().take(limit).enumerate() {
        let start = Instant::now();
        let composed = client
            .append_text(AppendTextRequest {
                text_to_append: ch.to_string(),
                preview_only: !legacy,
            })
            .await?
            .into_inner()
            .composing_text
            .ok_or("Missing composing text")?;
        timings.push(start.elapsed().as_secs_f64() * 1000.0);
        if !legacy {
            assert!(
                composed.suggestions.is_empty(),
                "Typing generated conversions"
            );
        }
        reading = composed.hiragana;
        if !legacy && (index + 1) % 12 == 0 {
            let mut predictor = client.clone();
            let request = ConvertTextRequest {
                reading: reading.clone(),
                raw_input: composed.raw_input,
                context: String::new(),
                prediction_only: true,
            };
            predictions.push(tokio::spawn(async move {
                let response = predictor.convert_text(request).await.unwrap().into_inner();
                assert!(response
                    .composing_text
                    .unwrap()
                    .suggestions
                    .iter()
                    .all(|s| s.is_prediction));
            }));
        }
        if [10, 30, 60, 120, 240].contains(&(index + 1)) {
            let mut sorted = timings.clone();
            sorted.sort_by(f64::total_cmp);
            batches.push(serde_json::json!({
                "raw_characters": index + 1, "reading_characters": reading.chars().count(),
                "median_ms": sorted[sorted.len() / 2], "p95_ms": sorted[(sorted.len() - 1) * 95 / 100],
                "max_ms": sorted.last(),
            }));
        }
        if !legacy {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    for prediction in predictions {
        prediction.await?;
    }
    let mut checks = Vec::new();
    if !legacy {
        client
            .clear_text(ClearTextRequest { preview_only: true })
            .await?;
        let mut prefix = String::new();
        for ch in "Windows".chars() {
            prefix.push(ch);
            let response = client
                .append_text(AppendTextRequest {
                    text_to_append: ch.to_string(),
                    preview_only: true,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert_eq!(response.hiragana, prefix);
            assert_eq!(response.raw_input, prefix);
        }
        checks.push("Windows remains Latin at every keystroke");
        for input in ["windows", "kanji", "kyou", "kode"] {
            client
                .clear_text(ClearTextRequest { preview_only: true })
                .await?;
            let composed = client
                .append_text(AppendTextRequest {
                    text_to_append: input.into(),
                    preview_only: true,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            let prediction = input == "kode";
            let result = client
                .convert_text(ConvertTextRequest {
                    reading: composed.hiragana,
                    raw_input: composed.raw_input,
                    context: String::new(),
                    prediction_only: prediction,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert!(result
                .suggestions
                .iter()
                .all(|s| s.is_prediction == prediction));
            let expected = match input {
                "windows" => "windows",
                "kanji" => "漢字",
                "kyou" => "今日",
                _ => "検証専用語",
            };
            assert!(
                result.suggestions.iter().any(|s| s.text == expected),
                "Missing {expected}: {:?}",
                result.suggestions
            );
            if input == "kyou" {
                assert_eq!(result.suggestions[0].text, "今日");
            }
        }
        checks.push("Space returns normal candidates including typed English");
        checks.push("Tab returns predictions only");
        checks.push("Concurrent predictions never add candidates to typing responses");
    }
    client
        .clear_text(ClearTextRequest {
            preview_only: !legacy,
        })
        .await?;
    println!(
        "{}",
        serde_json::json!({ "legacy": legacy, "batches": batches, "checks": checks, "final_reading_length": reading.chars().count() })
    );
    Ok(())
}
