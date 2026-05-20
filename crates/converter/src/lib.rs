use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, Timelike};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_DICTIONARY_CANDIDATES_PER_KEY: usize = 32;
const MAX_RETURNED_CANDIDATES: usize = 16;
const DEFAULT_ZENZAI_MODEL: &str = "zenz.gguf";
const DEFAULT_ZENZAI_COMMAND: &str = "llama-cli.exe";

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub text: String,
    pub subtext: String,
    pub corresponding_count: i32,
}

#[derive(Clone, Debug)]
struct DictionaryEntry {
    text: String,
    score: f32,
}

#[derive(Clone, Debug)]
struct CompoundCandidate {
    text: String,
    score: f32,
}

#[derive(Clone, Debug)]
pub struct ZenzaiConfig {
    pub enabled: bool,
    pub model_path: PathBuf,
    pub command_path: PathBuf,
    pub profile: String,
    pub personalization_enabled: bool,
    pub personalization_path: PathBuf,
    pub inference_limit: usize,
    pub timeout_ms: u64,
    pub prediction_enabled: bool,
    pub prediction_token_limit: usize,
}

#[derive(Clone, Debug)]
pub struct MagicConversionConfig {
    pub enabled: bool,
    pub command_path: PathBuf,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug)]
pub struct ConversionConfig {
    pub learning_enabled: bool,
    pub prediction_enabled: bool,
    pub live_conversion_enabled: bool,
    pub typo_correction_enabled: bool,
    pub input_style: String,
    pub custom_input_table_path: Option<PathBuf>,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            learning_enabled: true,
            prediction_enabled: true,
            live_conversion_enabled: true,
            typo_correction_enabled: true,
            input_style: "default".to_string(),
            custom_input_table_path: None,
        }
    }
}

#[derive(Debug)]
struct InputTable {
    entries: Vec<(String, String)>,
}

impl Default for InputTable {
    fn default() -> Self {
        Self::standard()
    }
}

impl ZenzaiConfig {
    pub fn disabled(model_path: PathBuf, command_path: PathBuf) -> Self {
        Self {
            enabled: false,
            model_path,
            command_path,
            profile: String::new(),
            personalization_enabled: false,
            personalization_path: PathBuf::new(),
            inference_limit: 1,
            timeout_ms: 1500,
            prediction_enabled: false,
            prediction_token_limit: 32,
        }
    }
}

impl Default for MagicConversionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command_path: PathBuf::new(),
            timeout_ms: 1500,
        }
    }
}

#[derive(Debug, Default)]
struct Dictionary {
    entries: HashMap<String, Vec<DictionaryEntry>>,
}

pub struct NativeConverter {
    dictionary: Dictionary,
    user_dictionary: UserDictionary,
    emoji_dictionary: EmojiDictionary,
    learning: LearningStore,
    raw_input: String,
    hiragana: String,
    context: String,
    conversion_config: ConversionConfig,
    zenzai_config: ZenzaiConfig,
    zenzai: ZenzaiEngine,
    input_table: InputTable,
    magic_conversion_config: MagicConversionConfig,
    magic_conversion: MagicConversionEngine,
}

impl Default for NativeConverter {
    fn default() -> Self {
        Self {
            dictionary: Dictionary::default(),
            user_dictionary: UserDictionary::load(default_user_dictionary_path()),
            emoji_dictionary: EmojiDictionary::default(),
            learning: LearningStore::load(default_learning_path()),
            raw_input: String::new(),
            hiragana: String::new(),
            context: String::new(),
            conversion_config: ConversionConfig::default(),
            zenzai_config: ZenzaiConfig::disabled(
                PathBuf::from(DEFAULT_ZENZAI_MODEL),
                PathBuf::from(DEFAULT_ZENZAI_COMMAND),
            ),
            zenzai: ZenzaiEngine::default(),
            input_table: InputTable::default(),
            magic_conversion_config: MagicConversionConfig::default(),
            magic_conversion: MagicConversionEngine::default(),
        }
    }
}

impl NativeConverter {
    pub fn load(resource_dir: impl AsRef<Path>) -> Self {
        let dictionary = Dictionary::load(resource_dir.as_ref().join("Dictionary"));
        let user_dictionary = UserDictionary::load(default_user_dictionary_path());
        let emoji_dictionary = EmojiDictionary::load(resource_dir.as_ref().join("EmojiDictionary"));
        let learning = LearningStore::load(default_learning_path());
        let zenzai_config = ZenzaiConfig::disabled(
            resource_dir.as_ref().join(DEFAULT_ZENZAI_MODEL),
            resource_dir.as_ref().join(DEFAULT_ZENZAI_COMMAND),
        );
        Self {
            dictionary,
            user_dictionary,
            emoji_dictionary,
            learning,
            zenzai_config,
            ..Default::default()
        }
    }

    pub fn configure_zenzai(&mut self, config: ZenzaiConfig) {
        self.zenzai_config = config;
        self.zenzai.reset();
    }

    pub fn configure_magic_conversion(&mut self, config: MagicConversionConfig) {
        self.magic_conversion_config = config;
    }

    pub fn configure_conversion(&mut self, config: ConversionConfig) {
        self.input_table = InputTable::from_config(&config);
        self.conversion_config = config;
        self.refresh_hiragana();
    }

    pub fn reload_user_dictionary(&mut self) {
        self.user_dictionary = UserDictionary::load(default_user_dictionary_path());
    }

    pub fn reload_learning(&mut self) {
        self.learning = LearningStore::load(default_learning_path());
    }

    pub fn append_text(&mut self, input: &str) -> Vec<Candidate> {
        self.raw_input.push_str(input);
        self.refresh_hiragana();
        self.candidates()
    }

    pub fn remove_text(&mut self) -> Vec<Candidate> {
        self.raw_input.pop();
        self.refresh_hiragana();
        self.candidates()
    }

    pub fn clear_text(&mut self) {
        self.raw_input.clear();
        self.hiragana.clear();
    }

    pub fn shrink_text(&mut self, count: i32) -> Vec<Candidate> {
        let count = count.max(0) as usize;
        self.hiragana = skip_chars(&self.hiragana, count);
        self.raw_input = self.hiragana.clone();
        self.candidates()
    }

    pub fn set_context(&mut self, context: impl Into<String>) {
        self.context = context.into();
    }

    pub fn record_commit(&mut self, reading: &str, text: &str) {
        if !self.conversion_config.learning_enabled {
            return;
        }
        self.learning.record_commit(reading, text);
    }

    pub fn hiragana(&self) -> &str {
        &self.hiragana
    }

    pub fn candidates(&mut self) -> Vec<Candidate> {
        if self.hiragana.is_empty() {
            return Vec::new();
        }

        let mut candidates = self.user_dictionary.lookup_best_prefixes(&self.hiragana);
        candidates.extend(DynamicDictionary::lookup_best_prefixes(&self.hiragana));
        candidates.extend(self.magic_conversion.candidates(
            &self.magic_conversion_config,
            &self.context,
            &self.hiragana,
        ));
        candidates.extend(self.zenzai.prediction_candidates(
            &self.zenzai_config,
            &self.context,
            &self.hiragana,
        ));
        candidates.extend(self.emoji_dictionary.lookup_best_prefixes(&self.hiragana));
        if self.conversion_config.learning_enabled {
            candidates.extend(self.learning.lookup_best_prefixes(&self.hiragana));
        }
        if self.conversion_config.live_conversion_enabled {
            candidates.extend(self.dictionary.lookup_compound_candidates(&self.hiragana));
            let dictionary_candidates = self.dictionary.lookup_best_prefixes(&self.hiragana);
            let has_full_dictionary_match = dictionary_candidates.iter().any(|candidate| {
                candidate.corresponding_count as usize == self.hiragana.chars().count()
            });
            candidates.extend(dictionary_candidates);
            if self.conversion_config.typo_correction_enabled && !has_full_dictionary_match {
                candidates.extend(self.dictionary.lookup_typo_corrections(&self.hiragana));
            }
        }
        if self.conversion_config.prediction_enabled {
            candidates.extend(self.user_dictionary.lookup_predictions(&self.hiragana));
            candidates.extend(DynamicDictionary::lookup_predictions(&self.hiragana));
            candidates.extend(self.emoji_dictionary.lookup_predictions(&self.hiragana));
            if self.conversion_config.learning_enabled {
                candidates.extend(self.learning.lookup_predictions(&self.hiragana));
            }
            candidates.extend(self.dictionary.lookup_predictions(&self.hiragana));
        }
        candidates.push(Candidate {
            text: self.hiragana.clone(),
            subtext: String::new(),
            corresponding_count: self.hiragana.chars().count() as i32,
        });
        let candidates = dedupe_candidates(candidates);
        self.zenzai.rerank(
            &self.zenzai_config,
            &self.context,
            &self.hiragana,
            candidates,
        )
    }

    fn refresh_hiragana(&mut self) {
        self.hiragana = roman_to_hiragana_with_entries(&self.raw_input, &self.input_table.entries);
    }
}

impl InputTable {
    fn standard() -> Self {
        Self::from_static(ROMAJI_TABLE)
    }

    fn azik() -> Self {
        let mut entries = azik_entries();
        entries.extend(
            ROMAJI_TABLE
                .iter()
                .map(|(romaji, kana)| ((*romaji).to_string(), (*kana).to_string())),
        );
        sort_input_entries(&mut entries);
        Self { entries }
    }

    fn custom(path: Option<PathBuf>) -> Self {
        let mut entries = path
            .and_then(|path| load_custom_input_table(&path))
            .unwrap_or_default();
        entries.extend(
            ROMAJI_TABLE
                .iter()
                .map(|(romaji, kana)| ((*romaji).to_string(), (*kana).to_string())),
        );
        sort_input_entries(&mut entries);
        Self { entries }
    }

    fn from_config(config: &ConversionConfig) -> Self {
        match config.input_style.as_str() {
            "azik" => Self::azik(),
            "custom" => Self::custom(
                config
                    .custom_input_table_path
                    .clone()
                    .or_else(default_input_table_path),
            ),
            _ => Self::standard(),
        }
    }

    fn from_static(entries: &[(&str, &str)]) -> Self {
        let mut entries = entries
            .iter()
            .map(|(romaji, kana)| ((*romaji).to_string(), (*kana).to_string()))
            .collect::<Vec<_>>();
        sort_input_entries(&mut entries);
        Self { entries }
    }
}

struct DynamicDictionary;

impl DynamicDictionary {
    fn lookup_best_prefixes(hiragana: &str) -> Vec<Candidate> {
        Self::lookup_best_prefixes_at(hiragana, Local::now().naive_local())
    }

    fn lookup_predictions(hiragana: &str) -> Vec<Candidate> {
        Self::lookup_predictions_at(hiragana, Local::now().naive_local())
    }

    fn lookup_best_prefixes_at(hiragana: &str, now: NaiveDateTime) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut candidates = Vec::new();

        for prefix_len in (1..=length).rev() {
            let prefix = take_chars(hiragana, prefix_len);
            let subtext = skip_chars(hiragana, prefix_len);
            for text in dynamic_texts_for_reading(&prefix, now) {
                candidates.push(Candidate {
                    text,
                    subtext: subtext.clone(),
                    corresponding_count: prefix_len as i32,
                });
                if candidates.len() >= MAX_RETURNED_CANDIDATES {
                    return candidates;
                }
            }
        }

        candidates
    }

    fn lookup_predictions_at(hiragana: &str, now: NaiveDateTime) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        dynamic_readings()
            .iter()
            .filter(|reading| reading.starts_with(hiragana) && **reading != hiragana)
            .flat_map(|reading| dynamic_texts_for_reading(reading, now))
            .map(|text| Candidate {
                text,
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }
}

fn dynamic_readings() -> &'static [&'static str] {
    &[
        "いま",
        "きょう",
        "ほんじつ",
        "あした",
        "あす",
        "みょうにち",
        "あさって",
        "きのう",
        "さくじつ",
        "おととい",
        "ことし",
        "らいねん",
        "きょねん",
    ]
}

fn dynamic_texts_for_reading(reading: &str, now: NaiveDateTime) -> Vec<String> {
    let date = now.date();
    match reading {
        "いま" => time_candidates(now),
        "きょう" | "ほんじつ" => date_candidates(date),
        "あした" | "あす" | "みょうにち" => {
            date_candidates(date + ChronoDuration::days(1))
        }
        "あさって" => date_candidates(date + ChronoDuration::days(2)),
        "きのう" | "さくじつ" => date_candidates(date - ChronoDuration::days(1)),
        "おととい" => date_candidates(date - ChronoDuration::days(2)),
        "ことし" => vec![date.year().to_string()],
        "らいねん" => vec![(date.year() + 1).to_string()],
        "きょねん" => vec![(date.year() - 1).to_string()],
        _ => Vec::new(),
    }
}

fn time_candidates(now: NaiveDateTime) -> Vec<String> {
    vec![
        format!("{:02}:{:02}", now.hour(), now.minute()),
        format!("{}時{}分", now.hour(), now.minute()),
    ]
}

fn date_candidates(date: NaiveDate) -> Vec<String> {
    let weekday = japanese_weekday(date.weekday().num_days_from_sunday() as usize);
    let era_year = date.year() - 2018;
    vec![
        format!("{}年{}月{}日", date.year(), date.month(), date.day()),
        format!("{}/{}/{}", date.year(), date.month(), date.day()),
        format!("{}月{}日", date.month(), date.day()),
        format!("{}曜日", weekday),
        format!("令和{}年{}月{}日", era_year, date.month(), date.day()),
    ]
}

fn japanese_weekday(index_from_sunday: usize) -> &'static str {
    ["日", "月", "火", "水", "木", "金", "土"][index_from_sunday]
}

#[derive(Default)]
struct ZenzaiEngine;

#[derive(Default)]
struct MagicConversionEngine;

impl MagicConversionEngine {
    fn candidates(
        &mut self,
        config: &MagicConversionConfig,
        context: &str,
        hiragana: &str,
    ) -> Vec<Candidate> {
        if !config.enabled
            || hiragana.chars().count() < 2
            || config.command_path.as_os_str().is_empty()
        {
            return Vec::new();
        }

        let Some(output) = run_magic_conversion_command(config, context, hiragana) else {
            return Vec::new();
        };

        magic_candidates_from_output(&output, hiragana)
    }
}

fn magic_candidates_from_output(output: &str, hiragana: &str) -> Vec<Candidate> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_RETURNED_CANDIDATES)
        .map(|text| Candidate {
            text: text.to_string(),
            subtext: String::new(),
            corresponding_count: hiragana.chars().count() as i32,
        })
        .collect()
}

impl ZenzaiEngine {
    fn reset(&mut self) {}

    fn prediction_candidates(
        &mut self,
        config: &ZenzaiConfig,
        context: &str,
        hiragana: &str,
    ) -> Vec<Candidate> {
        if !config.enabled
            || !config.prediction_enabled
            || hiragana.chars().count() < 2
            || !config.model_path.exists()
        {
            return Vec::new();
        }

        let prompt = format!(
            "あなたは日本語IMEの予測変換エンジンです。\n文脈: {}\nプロフィール: {}\nパーソナライズ情報: {}\n読み: {}\n自然な変換候補を最大3個、1行1候補で出力してください。説明は不要です。",
            context,
            config.profile,
            personalization_text(config),
            hiragana
        );
        let Some(output) = run_zenzai_command(config, prompt, config.prediction_token_limit.max(4))
        else {
            return Vec::new();
        };

        zenzai_candidates_from_output(&output, hiragana)
    }

    fn rerank(
        &mut self,
        config: &ZenzaiConfig,
        context: &str,
        hiragana: &str,
        candidates: Vec<Candidate>,
    ) -> Vec<Candidate> {
        if !config.enabled || candidates.len() <= 1 || !config.model_path.exists() {
            return candidates;
        }

        let numbered = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| format!("{}: {}", index + 1, candidate.text))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "あなたは日本語IMEの変換候補を選ぶエンジンです。\n文脈: {}\nプロフィール: {}\nパーソナライズ情報: {}\n読み: {}\n候補:\n{}\n最も自然な候補の番号だけを1つ出力してください。",
            context,
            config.profile,
            personalization_text(config),
            hiragana,
            numbered
        );

        let Some(output) = run_zenzai_command(config, prompt, config.inference_limit.max(1)) else {
            return candidates;
        };

        let Some(index) = output
            .chars()
            .find(|ch| ch.is_ascii_digit())
            .and_then(|ch| ch.to_digit(10))
            .and_then(|value| value.checked_sub(1))
            .map(|value| value as usize)
        else {
            return candidates;
        };

        promote_candidate(candidates, index)
    }
}

fn run_magic_conversion_command(
    config: &MagicConversionConfig,
    context: &str,
    hiragana: &str,
) -> Option<String> {
    let mut child = Command::new(&config.command_path)
        .arg("--reading")
        .arg(hiragana)
        .arg("--context")
        .arg(context)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    read_child_with_timeout(&mut child, config.timeout_ms)
}

fn zenzai_candidates_from_output(output: &str, hiragana: &str) -> Vec<Candidate> {
    output
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|ch: char| {
                    ch.is_ascii_digit() || ch == '.' || ch == ')' || ch == '、'
                })
                .trim()
        })
        .filter(|line| !line.is_empty())
        .take(3)
        .map(|text| Candidate {
            text: text.to_string(),
            subtext: String::new(),
            corresponding_count: hiragana.chars().count() as i32,
        })
        .collect()
}

fn personalization_text(config: &ZenzaiConfig) -> String {
    if !config.personalization_enabled || config.personalization_path.as_os_str().is_empty() {
        return String::new();
    }

    fs::read_to_string(&config.personalization_path)
        .map(|content| content.chars().take(4096).collect())
        .unwrap_or_default()
}

fn run_zenzai_command(config: &ZenzaiConfig, prompt: String, limit: usize) -> Option<String> {
    let mut child = Command::new(&config.command_path)
        .arg("-m")
        .arg(&config.model_path)
        .arg("-n")
        .arg(limit.to_string())
        .arg("--temp")
        .arg("0")
        .arg("--no-display-prompt")
        .arg("-p")
        .arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    read_child_with_timeout(&mut child, config.timeout_ms)
}

fn read_child_with_timeout(child: &mut std::process::Child, timeout_ms: u64) -> Option<String> {
    let started_at = Instant::now();
    let timeout = Duration::from_millis(timeout_ms.max(100));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return Some(output);
            }
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
}

fn promote_candidate(mut candidates: Vec<Candidate>, index: usize) -> Vec<Candidate> {
    if index == 0 || index >= candidates.len() {
        return candidates;
    }
    let selected = candidates.remove(index);
    candidates.insert(0, selected);
    candidates
}

impl Dictionary {
    fn load(dictionary_dir: PathBuf) -> Self {
        let p_dir = dictionary_dir.join("p");
        let Ok(files) = fs::read_dir(p_dir) else {
            return Self::default();
        };

        let mut entries: HashMap<String, Vec<DictionaryEntry>> = HashMap::new();
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("csv") {
                continue;
            }
            Self::load_csv(&path, &mut entries);
        }

        for values in entries.values_mut() {
            values.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.text.cmp(&b.text))
            });
            let mut seen = HashSet::new();
            values.retain(|entry| seen.insert(entry.text.clone()));
            values.truncate(MAX_DICTIONARY_CANDIDATES_PER_KEY);
        }

        Self { entries }
    }

    fn load_csv(path: &Path, entries: &mut HashMap<String, Vec<DictionaryEntry>>) {
        let Ok(file) = fs::File::open(path) else {
            return;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let fields = parse_csv_line(&line);
            let Some(reading) = fields.first() else {
                continue;
            };
            let Some(text) = fields.get(1) else {
                continue;
            };
            let score = fields
                .get(5)
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or_default();

            entries
                .entry(katakana_to_hiragana(reading))
                .or_default()
                .push(DictionaryEntry {
                    text: text.to_string(),
                    score,
                });
        }
    }

    fn lookup_best_prefixes(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut candidates = Vec::new();

        for prefix_len in (1..=length).rev() {
            let prefix = take_chars(hiragana, prefix_len);
            let Some(entries) = self.entries.get(&prefix) else {
                continue;
            };
            let subtext = skip_chars(hiragana, prefix_len);
            for entry in entries {
                candidates.push(Candidate {
                    text: entry.text.clone(),
                    subtext: subtext.clone(),
                    corresponding_count: prefix_len as i32,
                });
                if candidates.len() >= MAX_RETURNED_CANDIDATES {
                    return candidates;
                }
            }
        }

        candidates
    }

    fn lookup_compound_candidates(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        if !(2..=16).contains(&length) {
            return Vec::new();
        }

        let mut memo = HashMap::new();
        let mut compounds = self.compound_candidates_from(hiragana, 0, length, &mut memo);
        compounds.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
        });
        compounds
            .into_iter()
            .map(|candidate| Candidate {
                text: candidate.text,
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }

    fn compound_candidates_from(
        &self,
        hiragana: &str,
        start: usize,
        length: usize,
        memo: &mut HashMap<usize, Vec<CompoundCandidate>>,
    ) -> Vec<CompoundCandidate> {
        if start == length {
            return vec![CompoundCandidate {
                text: String::new(),
                score: 0.0,
            }];
        }
        if let Some(candidates) = memo.get(&start) {
            return candidates.clone();
        }

        let mut candidates = Vec::new();
        for end in ((start + 1)..=length).rev() {
            let key = slice_chars(hiragana, start, end);
            let Some(entries) = self.entries.get(&key) else {
                continue;
            };
            let tails = self.compound_candidates_from(hiragana, end, length, memo);
            if tails.is_empty() {
                continue;
            }

            for entry in entries.iter().take(3) {
                for tail in tails.iter().take(3) {
                    let mut text = entry.text.clone();
                    text.push_str(&tail.text);
                    candidates.push(CompoundCandidate {
                        text,
                        score: entry.score + tail.score,
                    });
                }
            }
        }

        candidates.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
        });
        candidates.truncate(MAX_RETURNED_CANDIDATES);
        memo.insert(start, candidates.clone());
        candidates
    }

    fn lookup_predictions(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut entries = self
            .entries
            .iter()
            .filter(|(reading, _)| reading.starts_with(hiragana) && reading.as_str() != hiragana)
            .flat_map(|(_, entries)| entries.iter())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
        });

        entries
            .into_iter()
            .map(|entry| Candidate {
                text: entry.text.clone(),
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }

    fn lookup_typo_corrections(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        if length < 3 {
            return Vec::new();
        }

        let first = hiragana.chars().next();
        let mut entries = self
            .entries
            .iter()
            .filter(|(reading, _)| {
                reading.chars().next() == first && is_edit_distance_one_or_less(reading, hiragana)
            })
            .flat_map(|(_, entries)| entries.iter())
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
        });

        entries
            .into_iter()
            .map(|entry| Candidate {
                text: entry.text.clone(),
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => {
                quoted = !quoted;
            }
            ',' if !quoted => {
                fields.push(field);
                field = String::new();
            }
            _ => field.push(ch),
        }
    }
    fields.push(field);
    fields
}

#[derive(Debug, Default)]
struct UserDictionary {
    entries: HashMap<String, Vec<String>>,
}

impl UserDictionary {
    fn load(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            return Self::default();
        };
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };

        let mut entries: HashMap<String, Vec<String>> = HashMap::new();
        for line in content.lines() {
            let mut fields = line.splitn(3, '\t');
            let Some(reading) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(text) = fields.next().map(str::trim) else {
                continue;
            };
            if reading.is_empty() || text.is_empty() {
                continue;
            }
            let values = entries.entry(reading.to_string()).or_default();
            if !values.iter().any(|value| value == text) {
                values.push(text.to_string());
            }
        }

        Self { entries }
    }

    fn lookup_best_prefixes(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut candidates = Vec::new();

        for prefix_len in (1..=length).rev() {
            let prefix = take_chars(hiragana, prefix_len);
            let Some(entries) = self.entries.get(&prefix) else {
                continue;
            };
            let subtext = skip_chars(hiragana, prefix_len);
            for text in entries {
                candidates.push(Candidate {
                    text: text.clone(),
                    subtext: subtext.clone(),
                    corresponding_count: prefix_len as i32,
                });
                if candidates.len() >= MAX_RETURNED_CANDIDATES {
                    return candidates;
                }
            }
        }

        candidates
    }

    fn lookup_predictions(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        self.entries
            .iter()
            .filter(|(reading, _)| reading.starts_with(hiragana) && reading.as_str() != hiragana)
            .flat_map(|(_, entries)| entries.iter())
            .map(|text| Candidate {
                text: text.clone(),
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }
}

#[derive(Debug, Default)]
struct EmojiDictionary {
    entries: HashMap<String, Vec<String>>,
}

impl EmojiDictionary {
    fn load(dictionary_dir: PathBuf) -> Self {
        let Some(path) = latest_emoji_dictionary_path(&dictionary_dir) else {
            return Self::default();
        };
        Self::load_file(&path)
    }

    fn load_file(path: &Path) -> Self {
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };

        let mut entries: HashMap<String, Vec<String>> = HashMap::new();
        for line in content.lines() {
            let mut fields = line.split('\t');
            let Some(emoji) = fields.next().map(str::trim) else {
                continue;
            };
            let Some(keywords) = fields.next().map(str::trim) else {
                continue;
            };
            if emoji.is_empty() || keywords.is_empty() {
                continue;
            }
            for keyword in keywords.split(',').map(str::trim) {
                if keyword.is_empty() {
                    continue;
                }
                let values = entries.entry(keyword.to_string()).or_default();
                if !values.iter().any(|value| value == emoji) {
                    values.push(emoji.to_string());
                }
            }
        }

        Self { entries }
    }

    fn lookup_best_prefixes(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut candidates = Vec::new();

        for prefix_len in (1..=length).rev() {
            let prefix = take_chars(hiragana, prefix_len);
            let Some(entries) = self.entries.get(&prefix) else {
                continue;
            };
            let subtext = skip_chars(hiragana, prefix_len);
            for emoji in entries {
                candidates.push(Candidate {
                    text: emoji.clone(),
                    subtext: subtext.clone(),
                    corresponding_count: prefix_len as i32,
                });
                if candidates.len() >= MAX_RETURNED_CANDIDATES {
                    return candidates;
                }
            }
        }

        candidates
    }

    fn lookup_predictions(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        self.entries
            .iter()
            .filter(|(reading, _)| reading.starts_with(hiragana) && reading.as_str() != hiragana)
            .flat_map(|(_, entries)| entries.iter())
            .map(|emoji| Candidate {
                text: emoji.clone(),
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }
}

fn latest_emoji_dictionary_path(dictionary_dir: &Path) -> Option<PathBuf> {
    let mut files = fs::read_dir(dictionary_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("emoji_all_E") && name.ends_with(".txt"))
        })
        .collect::<Vec<_>>();
    files.sort();
    files.pop()
}

#[derive(Debug, Default)]
struct LearningStore {
    path: Option<PathBuf>,
    counts: HashMap<(String, String), u32>,
}

impl LearningStore {
    fn load(path: Option<PathBuf>) -> Self {
        let mut store = Self {
            path,
            counts: HashMap::new(),
        };
        let Some(path) = &store.path else {
            return store;
        };
        let Ok(content) = fs::read_to_string(path) else {
            return store;
        };

        for line in content.lines() {
            let mut fields = line.splitn(3, '\t');
            let Some(reading) = fields.next() else {
                continue;
            };
            let Some(text) = fields.next() else {
                continue;
            };
            let count = fields
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1);
            if !reading.is_empty() && !text.is_empty() {
                store
                    .counts
                    .insert((reading.to_string(), text.to_string()), count.max(1));
            }
        }

        store
    }

    fn record_commit(&mut self, reading: &str, text: &str) {
        let reading = reading.trim();
        let text = text.trim();
        if reading.is_empty() || text.is_empty() || reading == text {
            return;
        }

        *self
            .counts
            .entry((reading.to_string(), text.to_string()))
            .or_insert(0) += 1;
        let _ = self.persist();
    }

    fn lookup_best_prefixes(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut candidates = Vec::new();

        for prefix_len in (1..=length).rev() {
            let prefix = take_chars(hiragana, prefix_len);
            let subtext = skip_chars(hiragana, prefix_len);
            let mut entries = self
                .counts
                .iter()
                .filter_map(|((reading, text), count)| {
                    if reading == &prefix {
                        Some((text, count))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            entries.sort_by(|(left_text, left_count), (right_text, right_count)| {
                right_count
                    .cmp(left_count)
                    .then_with(|| left_text.cmp(right_text))
            });

            for (text, _) in entries {
                candidates.push(Candidate {
                    text: text.clone(),
                    subtext: subtext.clone(),
                    corresponding_count: prefix_len as i32,
                });
                if candidates.len() >= MAX_RETURNED_CANDIDATES {
                    return candidates;
                }
            }
        }

        candidates
    }

    fn lookup_predictions(&self, hiragana: &str) -> Vec<Candidate> {
        let length = hiragana.chars().count();
        let mut entries = self
            .counts
            .iter()
            .filter_map(|((reading, text), count)| {
                if reading.starts_with(hiragana) && reading != hiragana {
                    Some((text, count))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|(left_text, left_count), (right_text, right_count)| {
            right_count
                .cmp(left_count)
                .then_with(|| left_text.cmp(right_text))
        });

        entries
            .into_iter()
            .map(|(text, _)| Candidate {
                text: text.clone(),
                subtext: String::new(),
                corresponding_count: length as i32,
            })
            .take(MAX_RETURNED_CANDIDATES)
            .collect()
    }

    fn persist(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut rows = self.counts.iter().collect::<Vec<_>>();
        rows.sort_by(
            |((left_reading, left_text), _), ((right_reading, right_text), _)| {
                left_reading
                    .cmp(right_reading)
                    .then_with(|| left_text.cmp(right_text))
            },
        );
        let content = rows
            .into_iter()
            .map(|((reading, text), count)| format!("{reading}\t{text}\t{count}"))
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(path, content)
    }
}

fn default_learning_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Azookey").join("memory").join("learning.tsv"))
}

fn default_user_dictionary_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Azookey").join("user_dictionary.tsv"))
}

fn default_input_table_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Azookey").join("input_table.tsv"))
}

fn dedupe_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert((candidate.text.clone(), candidate.subtext.clone())))
        .take(MAX_RETURNED_CANDIDATES)
        .collect()
}

fn take_chars(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn skip_chars(value: &str, count: usize) -> String {
    value.chars().skip(count).collect()
}

fn slice_chars(value: &str, start: usize, end: usize) -> String {
    value.chars().skip(start).take(end - start).collect()
}

fn katakana_to_hiragana(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            let code = ch as u32;
            if (0x30A1..=0x30F6).contains(&code) {
                char::from_u32(code - 0x60).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

fn is_edit_distance_one_or_less(left: &str, right: &str) -> bool {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let left_len = left.len();
    let right_len = right.len();
    if left_len.abs_diff(right_len) > 1 {
        return false;
    }

    let mut left_index = 0;
    let mut right_index = 0;
    let mut edits = 0;
    while left_index < left_len && right_index < right_len {
        if left[left_index] == right[right_index] {
            left_index += 1;
            right_index += 1;
            continue;
        }

        edits += 1;
        if edits > 1 {
            return false;
        }

        match left_len.cmp(&right_len) {
            std::cmp::Ordering::Greater => left_index += 1,
            std::cmp::Ordering::Less => right_index += 1,
            std::cmp::Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
        }
    }

    edits + usize::from(left_index < left_len || right_index < right_len) <= 1
}

pub fn roman_to_hiragana(input: &str) -> String {
    roman_to_hiragana_with_entries(input, &InputTable::standard().entries)
}

fn roman_to_hiragana_with_entries(input: &str, entries: &[(String, String)]) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        let remaining: String = chars[index..]
            .iter()
            .take(8)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if let Some((kana, consumed)) = match_input_table(&remaining, entries) {
            result.push_str(&kana);
            index += consumed;
            continue;
        }

        if !ch.is_ascii_alphabetic() {
            result.push(ch);
            index += 1;
            continue;
        }

        let lower = ch.to_ascii_lowercase();
        if let Some(next) = chars.get(index + 1).copied() {
            if lower == next.to_ascii_lowercase()
                && lower != 'n'
                && is_consonant(lower)
                && next.is_ascii_alphabetic()
            {
                result.push('っ');
                index += 1;
                continue;
            }
        }

        if lower == 'n' {
            match chars.get(index + 1).map(|c| c.to_ascii_lowercase()) {
                Some('n') => {
                    result.push('ん');
                    index += 2;
                }
                Some(next) if is_vowel(next) || next == 'y' => {
                    result.push(ch);
                    index += 1;
                }
                _ => {
                    result.push('ん');
                    index += 1;
                }
            }
            continue;
        }

        result.push(ch);
        index += 1;
    }

    result
}

fn is_vowel(ch: char) -> bool {
    matches!(ch, 'a' | 'i' | 'u' | 'e' | 'o')
}

fn is_consonant(ch: char) -> bool {
    ch.is_ascii_alphabetic() && !is_vowel(ch)
}

fn match_input_table(input: &str, entries: &[(String, String)]) -> Option<(String, usize)> {
    entries
        .iter()
        .find(|(romaji, _)| input.starts_with(romaji.as_str()))
        .map(|(romaji, kana)| (kana.clone(), romaji.len()))
}

fn sort_input_entries(entries: &mut [(String, String)]) {
    entries.sort_by(|(left, _), (right, _)| {
        right.len().cmp(&left.len()).then_with(|| left.cmp(right))
    });
}

fn load_custom_input_table(path: &Path) -> Option<Vec<(String, String)>> {
    let content = fs::read_to_string(path).ok()?;
    let mut entries = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.splitn(2, '\t');
        let Some(romaji) = fields
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(kana) = fields
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        entries.push((romaji.to_ascii_lowercase(), kana.to_string()));
    }
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn azik_entries() -> Vec<(String, String)> {
    let mut entries = vec![
        ("q".to_string(), "ん".to_string()),
        (";".to_string(), "っ".to_string()),
        (":".to_string(), "ー".to_string()),
    ];

    for (stem, a, i, u, e, o) in AZIK_BASE_SYLLABLES {
        entries.push((format!("{stem}q"), format!("{a}い")));
        entries.push((format!("{stem}z"), format!("{a}ん")));
        entries.push((format!("{stem}k"), format!("{i}ん")));
        entries.push((format!("{stem}j"), format!("{u}ん")));
        entries.push((format!("{stem}d"), format!("{e}ん")));
        entries.push((format!("{stem}h"), format!("{o}ん")));
        entries.push((format!("{stem}p"), format!("{o}う")));
    }

    entries
}

const AZIK_BASE_SYLLABLES: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("k", "か", "き", "く", "け", "こ"),
    ("g", "が", "ぎ", "ぐ", "げ", "ご"),
    ("s", "さ", "し", "す", "せ", "そ"),
    ("z", "ざ", "じ", "ず", "ぜ", "ぞ"),
    ("t", "た", "ち", "つ", "て", "と"),
    ("d", "だ", "ぢ", "づ", "で", "ど"),
    ("n", "な", "に", "ぬ", "ね", "の"),
    ("h", "は", "ひ", "ふ", "へ", "ほ"),
    ("b", "ば", "び", "ぶ", "べ", "ぼ"),
    ("p", "ぱ", "ぴ", "ぷ", "ぺ", "ぽ"),
    ("m", "ま", "み", "む", "め", "も"),
    ("r", "ら", "り", "る", "れ", "ろ"),
];

const ROMAJI_TABLE: &[(&str, &str)] = &[
    ("xtsu", "っ"),
    ("ltsu", "っ"),
    ("kya", "きゃ"),
    ("kyu", "きゅ"),
    ("kyo", "きょ"),
    ("gya", "ぎゃ"),
    ("gyu", "ぎゅ"),
    ("gyo", "ぎょ"),
    ("sha", "しゃ"),
    ("shu", "しゅ"),
    ("sho", "しょ"),
    ("sya", "しゃ"),
    ("syu", "しゅ"),
    ("syo", "しょ"),
    ("ja", "じゃ"),
    ("ji", "じ"),
    ("ju", "じゅ"),
    ("jo", "じょ"),
    ("jya", "じゃ"),
    ("jyu", "じゅ"),
    ("jyo", "じょ"),
    ("cha", "ちゃ"),
    ("chu", "ちゅ"),
    ("cho", "ちょ"),
    ("cya", "ちゃ"),
    ("cyu", "ちゅ"),
    ("cyo", "ちょ"),
    ("nya", "にゃ"),
    ("nyu", "にゅ"),
    ("nyo", "にょ"),
    ("hya", "ひゃ"),
    ("hyu", "ひゅ"),
    ("hyo", "ひょ"),
    ("bya", "びゃ"),
    ("byu", "びゅ"),
    ("byo", "びょ"),
    ("pya", "ぴゃ"),
    ("pyu", "ぴゅ"),
    ("pyo", "ぴょ"),
    ("mya", "みゃ"),
    ("myu", "みゅ"),
    ("myo", "みょ"),
    ("rya", "りゃ"),
    ("ryu", "りゅ"),
    ("ryo", "りょ"),
    ("fa", "ふぁ"),
    ("fi", "ふぃ"),
    ("fe", "ふぇ"),
    ("fo", "ふぉ"),
    ("fyu", "ふゅ"),
    ("va", "ゔぁ"),
    ("vi", "ゔぃ"),
    ("vu", "ゔ"),
    ("ve", "ゔぇ"),
    ("vo", "ゔぉ"),
    ("tsa", "つぁ"),
    ("tsi", "つぃ"),
    ("tse", "つぇ"),
    ("tso", "つぉ"),
    ("thi", "てぃ"),
    ("the", "てぇ"),
    ("dhi", "でぃ"),
    ("dhe", "でぇ"),
    ("kwa", "くぁ"),
    ("gwa", "ぐぁ"),
    ("shi", "し"),
    ("chi", "ち"),
    ("tsu", "つ"),
    ("xtu", "っ"),
    ("ltu", "っ"),
    ("xa", "ぁ"),
    ("xi", "ぃ"),
    ("xu", "ぅ"),
    ("xe", "ぇ"),
    ("xo", "ぉ"),
    ("la", "ぁ"),
    ("li", "ぃ"),
    ("lu", "ぅ"),
    ("le", "ぇ"),
    ("lo", "ぉ"),
    ("xya", "ゃ"),
    ("xyu", "ゅ"),
    ("xyo", "ょ"),
    ("lya", "ゃ"),
    ("lyu", "ゅ"),
    ("lyo", "ょ"),
    ("ka", "か"),
    ("ki", "き"),
    ("ku", "く"),
    ("ke", "け"),
    ("ko", "こ"),
    ("ga", "が"),
    ("gi", "ぎ"),
    ("gu", "ぐ"),
    ("ge", "げ"),
    ("go", "ご"),
    ("sa", "さ"),
    ("si", "し"),
    ("su", "す"),
    ("se", "せ"),
    ("so", "そ"),
    ("za", "ざ"),
    ("zi", "じ"),
    ("zu", "ず"),
    ("ze", "ぜ"),
    ("zo", "ぞ"),
    ("ta", "た"),
    ("ti", "ち"),
    ("tu", "つ"),
    ("te", "て"),
    ("to", "と"),
    ("da", "だ"),
    ("di", "ぢ"),
    ("du", "づ"),
    ("de", "で"),
    ("do", "ど"),
    ("na", "な"),
    ("ni", "に"),
    ("nu", "ぬ"),
    ("ne", "ね"),
    ("no", "の"),
    ("ha", "は"),
    ("hi", "ひ"),
    ("fu", "ふ"),
    ("hu", "ふ"),
    ("he", "へ"),
    ("ho", "ほ"),
    ("ba", "ば"),
    ("bi", "び"),
    ("bu", "ぶ"),
    ("be", "べ"),
    ("bo", "ぼ"),
    ("pa", "ぱ"),
    ("pi", "ぴ"),
    ("pu", "ぷ"),
    ("pe", "ぺ"),
    ("po", "ぽ"),
    ("ma", "ま"),
    ("mi", "み"),
    ("mu", "む"),
    ("me", "め"),
    ("mo", "も"),
    ("ya", "や"),
    ("yu", "ゆ"),
    ("yo", "よ"),
    ("ra", "ら"),
    ("ri", "り"),
    ("ru", "る"),
    ("re", "れ"),
    ("ro", "ろ"),
    ("wa", "わ"),
    ("wo", "を"),
    ("a", "あ"),
    ("i", "い"),
    ("u", "う"),
    ("e", "え"),
    ("o", "お"),
    ("-", "ー"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_romaji_to_hiragana() {
        assert_eq!(roman_to_hiragana("tokyo"), "ときょ");
        assert_eq!(roman_to_hiragana("toukyou"), "とうきょう");
        assert_eq!(roman_to_hiragana("gakkou"), "がっこう");
        assert_eq!(roman_to_hiragana("kanji"), "かんじ");
    }

    #[test]
    fn parses_quoted_dictionary_csv_fields() {
        let fields = parse_csv_line(r#"カンジ,"漢,字",1,2,普通名詞,-12.5"#);
        assert_eq!(fields[0], "カンジ");
        assert_eq!(fields[1], "漢,字");
        assert_eq!(fields[5], "-12.5");

        let escaped = parse_csv_line(r#"ヨミ,"候補""A",1,2,普通名詞,-1"#);
        assert_eq!(escaped[1], "候補\"A");
    }

    #[test]
    fn converts_azik_input_to_hiragana() {
        let table = InputTable::azik();
        assert_eq!(roman_to_hiragana_with_entries("q", &table.entries), "ん");
        assert_eq!(roman_to_hiragana_with_entries("kz", &table.entries), "かん");
        assert_eq!(roman_to_hiragana_with_entries("kk", &table.entries), "きん");
        assert_eq!(roman_to_hiragana_with_entries("kp", &table.entries), "こう");
        assert_eq!(roman_to_hiragana_with_entries(";", &table.entries), "っ");
        assert_eq!(roman_to_hiragana_with_entries(":", &table.entries), "ー");
    }

    #[test]
    fn loads_custom_input_table_before_standard_entries() {
        let path =
            std::env::temp_dir().join(format!("azookey-input-table-{}.tsv", std::process::id()));
        fs::write(&path, "vv\tゔ\n# comment\nka\tヵ\n").unwrap();

        let table = InputTable::custom(Some(path.clone()));
        assert_eq!(roman_to_hiragana_with_entries("vv", &table.entries), "ゔ");
        assert_eq!(roman_to_hiragana_with_entries("ka", &table.entries), "ヵ");
        assert_eq!(roman_to_hiragana_with_entries("ki", &table.entries), "き");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn typo_correction_uses_nearby_dictionary_readings() {
        let mut entries = HashMap::new();
        entries.insert(
            "かんじ".to_string(),
            vec![DictionaryEntry {
                text: "漢字".to_string(),
                score: 10.0,
            }],
        );
        let mut converter = NativeConverter {
            dictionary: Dictionary { entries },
            ..Default::default()
        };

        let candidates = converter.append_text("kanhi");
        assert!(candidates.iter().any(|candidate| candidate.text == "漢字"));

        converter.configure_conversion(ConversionConfig {
            typo_correction_enabled: false,
            ..Default::default()
        });
        let candidates = converter.candidates();
        assert!(!candidates.iter().any(|candidate| candidate.text == "漢字"));
    }

    #[test]
    fn dictionary_combines_multiple_segments() {
        let mut entries = HashMap::new();
        entries.insert(
            "かく".to_string(),
            vec![DictionaryEntry {
                text: "書く".to_string(),
                score: 10.0,
            }],
        );
        entries.insert(
            "こと".to_string(),
            vec![DictionaryEntry {
                text: "こと".to_string(),
                score: 5.0,
            }],
        );
        let mut converter = NativeConverter {
            dictionary: Dictionary { entries },
            ..Default::default()
        };

        let candidates = converter.append_text("kakukoto");
        assert!(candidates.iter().any(|candidate| {
            candidate.text == "書くこと" && candidate.corresponding_count == 4
        }));
    }

    #[test]
    fn returns_raw_candidate_without_dictionary() {
        let mut converter = NativeConverter::default();
        let candidates = converter.append_text("kanji");
        assert_eq!(converter.hiragana(), "かんじ");
        assert_eq!(candidates[0].text, "かんじ");
    }

    #[test]
    fn learned_candidate_is_ranked_first() {
        let path =
            std::env::temp_dir().join(format!("azookey-learning-{}.tsv", std::process::id()));
        let _ = fs::remove_file(&path);

        let mut converter = NativeConverter {
            learning: LearningStore::load(Some(path.clone())),
            ..Default::default()
        };
        converter.record_commit("かんじ", "漢字");

        let candidates = converter.append_text("kanji");
        assert_eq!(candidates[0].text, "漢字");
        assert_eq!(candidates[0].corresponding_count, 3);

        let reloaded = LearningStore::load(Some(path.clone()));
        assert_eq!(
            reloaded
                .counts
                .get(&("かんじ".to_string(), "漢字".to_string())),
            Some(&1)
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn user_dictionary_is_ranked_before_learning() {
        let path = std::env::temp_dir().join(format!(
            "azookey-user-dictionary-{}.tsv",
            std::process::id()
        ));
        fs::write(&path, "かんじ\tユーザー漢字").unwrap();

        let mut converter = NativeConverter {
            user_dictionary: UserDictionary::load(Some(path.clone())),
            learning: LearningStore::default(),
            ..Default::default()
        };
        converter.record_commit("かんじ", "学習漢字");

        let candidates = converter.append_text("kanji");
        assert_eq!(candidates[0].text, "ユーザー漢字");
        assert!(candidates
            .iter()
            .any(|candidate| candidate.text == "学習漢字"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn user_dictionary_ignores_part_of_speech_column() {
        let path = std::env::temp_dir().join(format!(
            "azookey-user-dictionary-pos-{}.tsv",
            std::process::id()
        ));
        fs::write(&path, "かんじ\t漢字\t普通名詞").unwrap();

        let mut converter = NativeConverter {
            user_dictionary: UserDictionary::load(Some(path.clone())),
            ..Default::default()
        };
        let candidates = converter.append_text("kanji");
        assert_eq!(candidates[0].text, "漢字");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn prediction_can_be_disabled() {
        let path = std::env::temp_dir().join(format!(
            "azookey-user-dictionary-prediction-{}.tsv",
            std::process::id()
        ));
        fs::write(&path, "かんじ\t漢字").unwrap();

        let mut converter = NativeConverter {
            user_dictionary: UserDictionary::load(Some(path.clone())),
            ..Default::default()
        };
        converter.append_text("kan");
        assert!(converter
            .candidates()
            .iter()
            .any(|candidate| candidate.text == "漢字"));

        converter.configure_conversion(ConversionConfig {
            prediction_enabled: false,
            ..Default::default()
        });
        let candidates = converter.candidates();
        assert!(!candidates.iter().any(|candidate| candidate.text == "漢字"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn learning_can_be_disabled() {
        let mut converter = NativeConverter::default();
        converter.configure_conversion(ConversionConfig {
            learning_enabled: false,
            ..Default::default()
        });
        converter.record_commit("かんじ", "漢字");

        let candidates = converter.append_text("kanji");
        assert!(!candidates.iter().any(|candidate| candidate.text == "漢字"));
    }

    #[test]
    fn emoji_dictionary_candidates_are_returned() {
        let path = std::env::temp_dir().join(format!(
            "azookey-emoji-dictionary-{}.txt",
            std::process::id()
        ));
        fs::write(&path, "😀\tえがお,すまいる\t\n").unwrap();

        let mut converter = NativeConverter {
            emoji_dictionary: EmojiDictionary::load_file(&path),
            ..Default::default()
        };

        let candidates = converter.append_text("egao");
        assert_eq!(candidates[0].text, "😀");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn dynamic_date_candidates_are_returned() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_opt(14, 5, 0)
            .unwrap();

        let candidates = DynamicDictionary::lookup_best_prefixes_at("きょうは", now);
        assert_eq!(candidates[0].text, "2026年5月20日");
        assert_eq!(candidates[0].subtext, "は");
        assert_eq!(candidates[0].corresponding_count, 3);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.text == "令和8年5月20日"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.text == "水曜日"));
    }

    #[test]
    fn dynamic_time_candidates_are_returned() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_opt(14, 5, 0)
            .unwrap();

        let candidates = DynamicDictionary::lookup_best_prefixes_at("いま", now);
        assert_eq!(candidates[0].text, "14:05");
        assert!(candidates
            .iter()
            .any(|candidate| candidate.text == "14時5分"));
    }

    #[test]
    fn dynamic_date_predictions_are_returned() {
        let now = NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_opt(14, 5, 0)
            .unwrap();

        let candidates = DynamicDictionary::lookup_predictions_at("きょ", now);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.text == "2026年5月20日"));
    }

    #[test]
    fn magic_conversion_parses_line_based_candidates() {
        let candidates = magic_candidates_from_output("いい感じの候補\n\n別候補\n", "よみ");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].text, "いい感じの候補");
        assert_eq!(candidates[0].corresponding_count, 2);
        assert_eq!(candidates[1].text, "別候補");
    }

    #[test]
    fn zenzai_prediction_parses_numbered_candidates() {
        let candidates = zenzai_candidates_from_output("1. 候補A\n2) 候補B\n", "よみ");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].text, "候補A");
        assert_eq!(candidates[0].corresponding_count, 2);
        assert_eq!(candidates[1].text, "候補B");
    }

    #[test]
    fn zenzai_personalization_reads_bounded_text() {
        let path = std::env::temp_dir().join(format!(
            "azookey-zenzai-personalization-{}.txt",
            std::process::id()
        ));
        fs::write(&path, "abcdef").unwrap();
        let config = ZenzaiConfig {
            personalization_enabled: true,
            personalization_path: path.clone(),
            ..ZenzaiConfig::disabled(PathBuf::new(), PathBuf::new())
        };

        assert_eq!(personalization_text(&config), "abcdef");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn packaged_dictionary_is_loaded() {
        let resource_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("server-swift")
            .join("azooKey_dictionary_storage");
        let mut converter = NativeConverter::load(resource_dir);

        let candidates = converter.append_text("kaku");
        assert!(candidates.iter().any(|candidate| candidate.text == "書く"));
    }
}
