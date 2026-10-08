use shared::{
    proto::{ComposingText, Suggestion},
    AppConfig,
};
use std::{
    collections::HashSet,
    ffi::{c_char, c_int, CStr, CString},
    path::{Path, PathBuf},
};

#[repr(C)]
struct FFICandidate {
    text: *mut c_char,
    subtext: *mut c_char,
    hiragana: *mut c_char,
    corresponding_count: c_int,
}

unsafe extern "C" {
    fn Initialize(path: *const c_char, use_zenzai: bool) -> *mut c_char;
    fn SetContext(context: *const c_char);
    fn AppendText(input: *const c_char, cursor: *mut c_int) -> *mut c_char;
    fn RemoveText(cursor: *mut c_int) -> *mut c_char;
    fn MoveCursor(offset: c_int, cursor: *mut c_int) -> *mut c_char;
    fn ShrinkText(offset: c_int) -> *mut c_char;
    fn ClearText();
    fn GetComposedText(length: *mut c_int) -> *mut *mut FFICandidate;
    fn LoadConfig() -> *mut c_char;
    fn ResetLearning();
    fn CommitCandidate(reading: *const c_char, text: *const c_char);
    fn FreeText(text: *mut c_char);
    fn FreeCandidates(candidates: *mut *mut FFICandidate, count: c_int);
    fn GetZenzaiStatus() -> *mut c_char;
}

// All access, including mutation and candidate collection, is serialized by the server mutex.
pub struct Engine {
    config: AppConfig,
    resource_dir: PathBuf,
    context: String,
    registered_words: HashSet<String>,
}

fn c_string(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| "Text contains a NUL character".to_string())
}

unsafe fn take_text(pointer: *mut c_char) -> String {
    let text = CStr::from_ptr(pointer).to_string_lossy().into_owned();
    FreeText(pointer);
    text
}

impl Engine {
    pub fn new(resource_dir: &Path) -> Result<Self, String> {
        let config = AppConfig::new();
        Self::validate(&config, resource_dir)?;
        let path = c_string(&resource_dir.to_string_lossy())?;
        let error = unsafe { take_text(Initialize(path.as_ptr(), true)) };
        if !error.is_empty() {
            return Err(error);
        }
        Ok(Self {
            config,
            resource_dir: resource_dir.to_path_buf(),
            context: String::new(),
            registered_words: Self::registered_words()?,
        })
    }

    fn registered_words() -> Result<HashSet<String>, String> {
        let path = PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA is not set")?)
            .join("Azookey/user_dictionary.tsv");
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(text
                .lines()
                .filter_map(|line| line.split('\t').nth(1))
                .map(str::to_string)
                .collect()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn validate(config: &AppConfig, resource_dir: &Path) -> Result<(), String> {
        if !(1..=100).contains(&config.conversion.max_candidates) {
            return Err("Candidate count must be between 1 and 100".to_string());
        }
        if config.zenzai.enable {
            let model = if config.zenzai.model_path.is_empty() {
                resource_dir.join("zenz.gguf")
            } else {
                PathBuf::from(&config.zenzai.model_path)
            };
            if !model.is_file() {
                return Err(format!("Zenzai model does not exist: {}", model.display()));
            }
            if !(1..=8).contains(&config.zenzai.inference_limit) {
                return Err("Zenzai inference limit must be between 1 and 8".to_string());
            }
        }
        if config.magic_conversion.enable
            && !Path::new(&config.magic_conversion.command_path).is_file()
        {
            return Err("Conversion provider executable does not exist".to_string());
        }
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let config = AppConfig::read();
        Self::validate(&config, &self.resource_dir)?;
        let error = unsafe { take_text(LoadConfig()) };
        if !error.is_empty() {
            return Err(error);
        }
        self.config = config;
        self.registered_words = Self::registered_words()?;
        Ok(())
    }

    pub fn append(&mut self, text: &str) -> Result<ComposingText, String> {
        let text = c_string(text)?;
        let mut cursor = 0;
        let reading = unsafe { take_text(AppendText(text.as_ptr(), &mut cursor)) };
        self.candidates(reading)
    }

    pub fn remove(&mut self) -> Result<ComposingText, String> {
        let mut cursor = 0;
        let reading = unsafe { take_text(RemoveText(&mut cursor)) };
        self.candidates(reading)
    }

    pub fn move_cursor(&mut self, offset: i32) -> Result<ComposingText, String> {
        let mut cursor = 0;
        let reading = unsafe { take_text(MoveCursor(offset, &mut cursor)) };
        self.candidates(reading)
    }

    pub fn shrink(&mut self, count: i32) -> Result<ComposingText, String> {
        if count < 0 {
            return Err("Shrink count cannot be negative".to_string());
        }
        let reading = unsafe { take_text(ShrinkText(count)) };
        self.candidates(reading)
    }

    pub fn clear(&mut self) {
        unsafe { ClearText() };
    }

    pub fn reset_learning(&mut self) {
        unsafe { ResetLearning() };
    }

    pub fn commit(&mut self, reading: &str, text: &str) -> Result<(), String> {
        let reading = c_string(reading)?;
        let text = c_string(text)?;
        unsafe { CommitCandidate(reading.as_ptr(), text.as_ptr()) };
        Ok(())
    }

    pub fn set_context(&mut self, text: &str) -> Result<(), String> {
        let text = text
            .rsplit(['\r', '\n'])
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        let context = c_string(text)?;
        unsafe { SetContext(context.as_ptr()) };
        self.context = text.to_string();
        Ok(())
    }

    fn candidates(&mut self, reading: String) -> Result<ComposingText, String> {
        if reading.is_empty() {
            return Ok(ComposingText {
                hiragana: reading,
                suggestions: vec![],
            });
        }
        let mut count = 0;
        let mut suggestions = Vec::new();
        unsafe {
            let pointer = GetComposedText(&mut count);
            for index in 0..count as usize {
                let candidate = &**pointer.add(index);
                suggestions.push(Suggestion {
                    text: CStr::from_ptr(candidate.text)
                        .to_string_lossy()
                        .into_owned(),
                    subtext: CStr::from_ptr(candidate.subtext)
                        .to_string_lossy()
                        .into_owned(),
                    corresponding_count: candidate.corresponding_count,
                });
            }
            FreeCandidates(pointer, count);
        }
        if self.config.zenzai.enable {
            let status = unsafe { take_text(GetZenzaiStatus()) };
            // The pinned converter appends four spaces and an error description on load failure.
            if status.contains("    ") {
                return Err(format!("Zenzai: {status}"));
            }
        }
        let mut supplemental = Vec::new();
        if self.config.conversion.dynamic_candidates {
            supplemental.extend(
                azookey_converter::dynamic_candidates(&reading)
                    .into_iter()
                    .map(|candidate| Suggestion {
                        text: candidate.text,
                        subtext: candidate.subtext,
                        corresponding_count: candidate.corresponding_count,
                    }),
            );
        }
        if self.config.magic_conversion.enable {
            let config = &self.config.magic_conversion;
            let extra = azookey_converter::external_candidates(
                Path::new(&config.command_path),
                config.timeout_ms,
                &self.context,
                &reading,
            )?;
            supplemental.extend(extra.into_iter().map(|candidate| Suggestion {
                text: candidate.text,
                subtext: candidate.subtext,
                corresponding_count: candidate.corresponding_count,
            }));
        }
        let registered_count = suggestions
            .iter()
            .take_while(|candidate| self.registered_words.contains(&candidate.text))
            .count();
        suggestions.splice(registered_count..registered_count, supplemental);
        suggestions.push(Suggestion {
            text: reading.clone(),
            subtext: String::new(),
            corresponding_count: reading.chars().count() as i32,
        });
        let mut seen = HashSet::new();
        suggestions.retain(|candidate| {
            seen.insert((candidate.text.clone(), candidate.corresponding_count))
        });
        suggestions.truncate(self.config.conversion.max_candidates);
        Ok(ComposingText {
            hiragana: reading,
            suggestions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A single test owns the Swift globals and a temporary APPDATA for its entire lifetime.
    #[test]
    fn official_engine_settings_dictionary_learning_and_ffi() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let appdata = root
            .join("target/verification")
            .join(format!("appdata-{}", std::process::id()));
        std::fs::create_dir_all(&appdata).unwrap();
        std::env::set_var("APPDATA", &appdata);
        let mut config = AppConfig::default();
        config.write();
        let resources = std::env::var_os("AZOOKEY_TEST_RESOURCES")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("server-swift/azooKey_dictionary_storage"));
        let mut engine = Engine::new(&resources).unwrap();
        let result = engine.append("kanji").unwrap();
        assert_eq!(result.hiragana, "かんじ");
        assert!(result
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "漢字"));
        assert!(result
            .suggestions
            .iter()
            .all(|candidate| candidate.corresponding_count <= 3));
        assert!(engine.append("\0").is_err());
        assert!(engine.shrink(-1).is_err());
        engine.clear();
        assert!(engine
            .append("kaku")
            .unwrap()
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "書く"));

        engine.clear();
        std::fs::write(
            appdata.join("Azookey/user_dictionary.tsv"),
            "こでっくす\t検証専用語\t固有名詞\n",
        )
        .unwrap();
        engine.reload().unwrap();
        let result = engine.append("kodekkusu").unwrap();
        assert_eq!(result.suggestions[0].text, "検証専用語");
        engine.clear();
        let result = engine.append("kode").unwrap();
        assert!(result
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証専用語"));
        engine.clear();
        config.conversion.prediction = false;
        config.write();
        engine.reload().unwrap();
        let result = engine.append("kode").unwrap();
        assert!(!result
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証専用語"));

        engine.clear();
        config.conversion.input_style = "azik".to_string();
        config.write();
        engine.reload().unwrap();
        assert_eq!(engine.append("kz").unwrap().hiragana, "かん");
        engine.clear();
        std::fs::write(appdata.join("Azookey/input_table.tsv"), "vv\tゔ\n").unwrap();
        config.conversion.input_style = "custom".to_string();
        config.write();
        engine.reload().unwrap();
        assert_eq!(engine.append("vvka").unwrap().hiragana, "ゔか");
        engine.clear();
        config.conversion.input_style = "default".to_string();
        config.write();
        engine.reload().unwrap();

        engine.reset_learning();
        engine.append("かんじ").unwrap();
        for _ in 0..5 {
            engine.commit("かんじ", "検証学習語").unwrap();
        }
        engine.clear();
        let result = engine.append("かんじ").unwrap();
        assert!(result
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証学習語"));
        engine.clear();
        config.learning.enable = false;
        config.write();
        engine.reload().unwrap();
        let result = engine.append("かんじ").unwrap();
        assert!(!result
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証学習語"));
        engine.clear();
        config.learning.enable = true;
        config.write();
        engine.reload().unwrap();
        engine.reset_learning();
        assert!(!engine
            .append("かんじ")
            .unwrap()
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証学習語"));

        engine.clear();
        let long = engine.append("nihongonyuuryoku").unwrap();
        assert_eq!(long.hiragana, "にほんごにゅうりょく");
        let shortened = engine.shrink(4).unwrap();
        assert_eq!(shortened.hiragana, "にゅうりょく");
        assert_eq!(engine.remove().unwrap().hiragana, "にゅうりょ");
        engine.move_cursor(-1).unwrap();
        engine.append("あ").unwrap();
        for _ in 0..200 {
            engine.clear();
            engine.append("kanji").unwrap();
        }
        engine.clear();
        config.conversion.max_candidates = 2;
        config.write();
        engine.reload().unwrap();
        assert_eq!(engine.append("kanji").unwrap().suggestions.len(), 2);

        engine.clear();
        config.conversion.max_candidates = 16;
        config.magic_conversion.enable = true;
        config.magic_conversion.command_path = root
            .join("target/debug/examples/provider_fixture.exe")
            .to_string_lossy()
            .into_owned();
        config.magic_conversion.timeout_ms = 100;
        config.write();
        engine.reload().unwrap();
        engine.set_context("前の行\n検証文脈").unwrap();
        assert!(engine
            .append("かんじ")
            .unwrap()
            .suggestions
            .iter()
            .any(|candidate| candidate.text == "検証文脈：かんじ"));
        engine.clear();
        assert!(engine
            .append("たいむあうと")
            .unwrap_err()
            .contains("timed out"));
        engine.clear();
        assert!(engine
            .append("しっぱい")
            .unwrap_err()
            .contains("provider failure"));
        engine.clear();
        assert!(engine.append("から").unwrap_err().contains("no candidates"));
        engine.clear();
        config.magic_conversion.enable = false;
        config.zenzai.personalization = true;
        config.zenzai.personalization_path =
            appdata.join("profile.txt").to_string_lossy().into_owned();
        std::fs::write(
            &config.zenzai.personalization_path,
            "検証用の専門用語を使います。",
        )
        .unwrap();
        config.write();
        engine.reload().unwrap();
        config.zenzai.personalization_path = "missing-profile.txt".to_string();
        config.zenzai.enable = true;
        config.zenzai.model_path = root.join("zenz.gguf").to_string_lossy().into_owned();
        config.write();
        assert!(engine.reload().is_err());

        if std::env::var_os("AZOOKEY_TEST_ZENZAI").is_some() {
            config.zenzai.personalization = false;
            config.zenzai.enable = true;
            config.zenzai.model_path = root.join("zenz.gguf").to_string_lossy().into_owned();
            config.write();
            engine.reload().unwrap();
            let result = engine.append("kyouhaiitenkidesu").unwrap();
            assert!(result
                .suggestions
                .iter()
                .any(|candidate| candidate.text.contains("天気")));
            let status = unsafe { take_text(GetZenzaiStatus()) };
            assert!(
                status.starts_with("load ") && !status.contains("    "),
                "{status}"
            );
            eprintln!("Zenzai real inference: {status}");
            engine.clear();
        }
    }
}
