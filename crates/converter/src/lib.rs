use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

const MAX_DICTIONARY_CANDIDATES_PER_KEY: usize = 32;
const MAX_RETURNED_CANDIDATES: usize = 16;

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

#[derive(Clone, Debug, Default)]
struct Dictionary {
    entries: HashMap<String, Vec<DictionaryEntry>>,
}

#[derive(Clone, Debug, Default)]
pub struct NativeConverter {
    dictionary: Dictionary,
    raw_input: String,
    hiragana: String,
    context: String,
}

impl NativeConverter {
    pub fn load(resource_dir: impl AsRef<Path>) -> Self {
        let dictionary = Dictionary::load(resource_dir.as_ref().join("Dictionary"));
        Self {
            dictionary,
            ..Default::default()
        }
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

    pub fn hiragana(&self) -> &str {
        &self.hiragana
    }

    pub fn candidates(&self) -> Vec<Candidate> {
        if self.hiragana.is_empty() {
            return Vec::new();
        }

        let mut candidates = self.dictionary.lookup_best_prefixes(&self.hiragana);
        candidates.push(Candidate {
            text: self.hiragana.clone(),
            subtext: String::new(),
            corresponding_count: self.hiragana.chars().count() as i32,
        });
        dedupe_candidates(candidates)
    }

    fn refresh_hiragana(&mut self) {
        self.hiragana = roman_to_hiragana(&self.raw_input);
    }
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
}
