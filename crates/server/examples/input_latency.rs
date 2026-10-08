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
        for (input, expected_reading, expected_text) in [
            (
                "windowswotsukaimasu",
                "windowsをつかいます",
                "windowsを使います",
            ),
            (
                "kyouhagoogledekensaku",
                "きょうはgoogleでけんさく",
                "今日はgoogleで検索",
            ),
            (
                "ashitameetinggaaru",
                "あしたmeetingがある",
                "明日meetingがある",
            ),
            ("pythonnobug", "pythonのbug", "pythonのbug"),
            ("kyouhagithub", "きょうはgithub", "今日はgithub"),
        ] {
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
            assert_eq!(composed.hiragana, expected_reading, "{input}");
            assert_eq!(composed.raw_input, input);
            assert!(composed.suggestions.is_empty());
            let result = client
                .convert_text(ConvertTextRequest {
                    reading: composed.hiragana,
                    raw_input: composed.raw_input,
                    context: String::new(),
                    prediction_only: false,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert!(
                result
                    .suggestions
                    .iter()
                    .any(|s| format!("{}{}", s.text, s.subtext) == expected_text),
                "Missing {expected_text}: {:?}",
                result.suggestions
            );
            let remaining = client
                .append_text(AppendTextRequest {
                    text_to_append: String::new(),
                    preview_only: true,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert_eq!(remaining.hiragana, expected_reading);
            assert_eq!(remaining.raw_input, input);
        }
        checks.push("Mixed English/Japanese readings and conversion candidates preserve Latin spans and raw strokes");
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
                let today = result
                    .suggestions
                    .iter()
                    .position(|s| s.text == "今日")
                    .unwrap();
                let date = result
                    .suggestions
                    .iter()
                    .position(|s| s.text.contains('年'))
                    .unwrap();
                assert!(
                    today < date,
                    "Date helpers outranked 今日: {:?}",
                    result.suggestions
                );
            }
        }
        checks.push("Space returns normal candidates including typed English");
        checks.push("Tab returns predictions only");
        checks.push("Concurrent predictions never add candidates to typing responses");
        client
            .clear_text(ClearTextRequest { preview_only: true })
            .await?;
        let original = client
            .append_text(AppendTextRequest {
                text_to_append: "kyouhaiitenkidesune".into(),
                preview_only: true,
            })
            .await?
            .into_inner()
            .composing_text
            .unwrap();
        let converted = client
            .convert_text(ConvertTextRequest {
                reading: original.hiragana.clone(),
                raw_input: original.raw_input.clone(),
                context: String::new(),
                prediction_only: false,
            })
            .await?
            .into_inner()
            .composing_text
            .unwrap();
        let selected = &converted.suggestions[0];
        assert!(
            selected.clauses.len() > 1,
            "Missing clause boundaries: {selected:?}"
        );
        let chars: Vec<_> = original.hiragana.chars().collect();
        let mut offset = 0;
        let mut context = String::new();
        for clause in &selected.clauses {
            let end = offset + clause.corresponding_count as usize;
            let reading: String = chars[offset..end].iter().collect();
            let candidates = client
                .convert_text(ConvertTextRequest {
                    reading,
                    raw_input: String::new(),
                    context: context.clone(),
                    prediction_only: false,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert!(candidates
                .suggestions
                .iter()
                .any(|candidate| !candidate.is_prediction
                    && candidate.corresponding_count == clause.corresponding_count
                    && candidate.subtext.is_empty()));
            context.push_str(&clause.text);
            offset = end;
        }
        for count in [
            selected.clauses[0].corresponding_count - 1,
            selected.clauses[0].corresponding_count + 1,
        ] {
            if count <= 0 || count as usize > chars.len() {
                continue;
            }
            let candidates = client
                .convert_text(ConvertTextRequest {
                    reading: chars[..count as usize].iter().collect(),
                    raw_input: String::new(),
                    context: String::new(),
                    prediction_only: false,
                })
                .await?
                .into_inner()
                .composing_text
                .unwrap();
            assert!(candidates
                .suggestions
                .iter()
                .any(|candidate| candidate.corresponding_count == count
                    && candidate.subtext.is_empty()));
        }
        let unchanged = client
            .append_text(AppendTextRequest {
                text_to_append: String::new(),
                preview_only: true,
            })
            .await?
            .into_inner()
            .composing_text
            .unwrap();
        assert_eq!(unchanged.hiragana, original.hiragana);
        assert_eq!(unchanged.raw_input, original.raw_input);
        checks.push("Clause queries and resized ranges preserve the complete uncommitted reading");
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
