use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
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
pub struct ZenzaiConfig {
    pub enabled: bool,
    pub model_path: PathBuf,
    pub command_path: PathBuf,
    pub profile: String,
    pub inference_limit: usize,
}

impl ZenzaiConfig {
    pub fn disabled(model_path: PathBuf, command_path: PathBuf) -> Self {
        Self {
            enabled: false,
            model_path,
            command_path,
            profile: String::new(),
            inference_limit: 1,
        }
    }
}

#[derive(Debug, Default)]
struct Dictionary {
    entries: HashMap<String, Vec<DictionaryEntry>>,
}

pub struct NativeConverter {
    dictionary: Dictionary,
    learning: LearningStore,
    raw_input: String,
    hiragana: String,
    context: String,
    zenzai_config: ZenzaiConfig,
    zenzai: ZenzaiEngine,
}

impl Default for NativeConverter {
    fn default() -> Self {
        Self {
            dictionary: Dictionary::default(),
            learning: LearningStore::load(default_learning_path()),
            raw_input: String::new(),
            hiragana: String::new(),
            context: String::new(),
            zenzai_config: ZenzaiConfig::disabled(
                PathBuf::from(DEFAULT_ZENZAI_MODEL),
                PathBuf::from(DEFAULT_ZENZAI_COMMAND),
            ),
            zenzai: ZenzaiEngine::default(),
        }
    }
}

impl NativeConverter {
    pub fn load(resource_dir: impl AsRef<Path>) -> Self {
        let dictionary = Dictionary::load(resource_dir.as_ref().join("Dictionary"));
        let learning = LearningStore::load(default_learning_path());
        let zenzai_config = ZenzaiConfig::disabled(
            resource_dir.as_ref().join(DEFAULT_ZENZAI_MODEL),
            resource_dir.as_ref().join(DEFAULT_ZENZAI_COMMAND),
        );
        Self {
            dictionary,
            learning,
            zenzai_config,
            ..Default::default()
        }
    }

    pub fn configure_zenzai(&mut self, config: ZenzaiConfig) {
        self.zenzai_config = config;
        self.zenzai.reset();
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
        self.learning.record_commit(reading, text);
    }

    pub fn hiragana(&self) -> &str {
        &self.hiragana
    }

    pub fn candidates(&mut self) -> Vec<Candidate> {
        if self.hiragana.is_empty() {
            return Vec::new();
        }

        let mut candidates = self.learning.lookup_best_prefixes(&self.hiragana);
        candidates.extend(self.dictionary.lookup_best_prefixes(&self.hiragana));
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
        self.hiragana = roman_to_hiragana(&self.raw_input);
    }
}

#[derive(Default)]
struct ZenzaiEngine;

impl ZenzaiEngine {
    fn reset(&mut self) {}

    fn rerank(
        &mut self,
        config: &ZenzaiConfig,
        context: &str,
        hiragana: &str,
        candidates: Vec<Candidate>,
    ) -> Vec<Candidate> {
        if !config.enabled
            || candidates.len() <= 1
            || !config.model_path.exists()
            || !config.command_path.exists()
        {
            return candidates;
        }

        let numbered = candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| format!("{}: {}", index + 1, candidate.text))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "あなたは日本語IMEの変換候補を選ぶエンジンです。\n文脈: {}\nプロフィール: {}\n読み: {}\n候補:\n{}\n最も自然な候補の番号だけを1つ出力してください。",
            context, config.profile, hiragana, numbered
        );

        let limit = config.inference_limit.max(1);
        let Ok(output) = Command::new(&config.command_path)
            .arg("-m")
            .arg(&config.model_path)
            .arg("-n")
            .arg(limit.to_string())
            .arg("--temp")
            .arg("0")
            .arg("--no-display-prompt")
            .arg("-p")
            .arg(prompt)
            .output()
        else {
            return candidates;
        };
        if !output.status.success() {
            return candidates;
        }

        let output = String::from_utf8_lossy(&output.stdout);

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
            let mut fields = line.splitn(6, ',');
            let Some(reading) = fields.next() else {
                continue;
            };
            let Some(text) = fields.next() else {
                continue;
            };
            let score = fields
                .nth(3)
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

pub fn roman_to_hiragana(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
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

        let remaining: String = chars[index..]
            .iter()
            .take(4)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if let Some((kana, consumed)) = match_romaji(&remaining) {
            result.push_str(kana);
            index += consumed;
            continue;
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

fn match_romaji(input: &str) -> Option<(&'static str, usize)> {
    ROMAJI_TABLE
        .iter()
        .find(|(romaji, _)| input.starts_with(*romaji))
        .map(|(romaji, kana)| (*kana, romaji.len()))
}

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
}
