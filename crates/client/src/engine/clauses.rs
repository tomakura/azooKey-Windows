use anyhow::{ensure, Result};

use super::ipc_service::Candidates;

#[derive(Clone, Debug)]
pub struct Clause {
    pub reading: String,
    pub text: String,
    pub pending: bool,
}

#[derive(Clone, Debug)]
pub struct ClauseSession {
    pub clauses: Vec<Clause>,
    pub active: usize,
}

impl ClauseSession {
    pub fn new(candidates: &Candidates, index: usize) -> Result<Self> {
        let text = candidates
            .texts
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("Missing selected candidate"))?;
        let count = candidates.corresponding_count[index] as usize;
        let reading: Vec<_> = candidates.hiragana.chars().collect();
        ensure!(
            count > 0 && count <= reading.len(),
            "Invalid candidate reading length"
        );
        let mut clauses = Vec::new();
        let mut offset = 0;
        for clause in candidates.clauses.get(index).into_iter().flatten() {
            let end = offset + clause.corresponding_count as usize;
            ensure!(
                end > offset && end <= count,
                "Invalid clause reading length"
            );
            clauses.push(Clause {
                reading: reading[offset..end].iter().collect(),
                text: clause.text.clone(),
                pending: false,
            });
            offset = end;
        }
        if clauses.is_empty() {
            clauses.push(Clause {
                reading: reading[..count].iter().collect(),
                text: text.clone(),
                pending: false,
            });
        } else {
            ensure!(
                offset == count
                    && clauses
                        .iter()
                        .map(|clause| clause.text.as_str())
                        .collect::<String>()
                        == *text,
                "Clause data does not match candidate"
            );
        }
        if count < reading.len() {
            clauses.push(Clause {
                reading: reading[count..].iter().collect(),
                text: candidates.sub_texts[index].clone(),
                pending: true,
            });
        }
        Ok(Self { clauses, active: 0 })
    }

    pub fn move_target(&mut self, offset: i32) {
        self.active =
            (self.active as i32 + offset).clamp(0, self.clauses.len() as i32 - 1) as usize;
    }

    pub fn resize(&mut self, offset: i32) -> bool {
        if offset < 0 {
            if self.clauses[self.active].reading.chars().count() <= 1 {
                return false;
            }
            let ch = self.clauses[self.active].reading.pop().unwrap();
            if self.active + 1 == self.clauses.len() {
                self.clauses.push(Clause {
                    reading: String::new(),
                    text: String::new(),
                    pending: true,
                });
            }
            self.clauses[self.active + 1].reading.insert(0, ch);
        } else {
            if self.active + 1 == self.clauses.len() {
                return false;
            }
            let ch = self.clauses[self.active + 1].reading.remove(0);
            self.clauses[self.active].reading.push(ch);
            if self.clauses[self.active + 1].reading.is_empty() {
                self.clauses.remove(self.active + 1);
            }
        }
        true
    }

    pub fn display(&self) -> (String, String, String) {
        (
            self.clauses[..self.active]
                .iter()
                .map(|clause| clause.text.as_str())
                .collect(),
            self.clauses[self.active].text.clone(),
            self.clauses[self.active + 1..]
                .iter()
                .map(|clause| clause.text.as_str())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::proto::ConversionClause;

    fn sentence() -> ClauseSession {
        ClauseSession::new(
            &Candidates {
                texts: vec!["今日は晴れ".into()],
                sub_texts: vec!["ですね".into()],
                hiragana: "きょうははれですね".into(),
                corresponding_count: vec![6],
                clauses: vec![vec![
                    ConversionClause {
                        text: "今日は".into(),
                        corresponding_count: 4,
                    },
                    ConversionClause {
                        text: "晴れ".into(),
                        corresponding_count: 2,
                    },
                ]],
                ..Candidates::default()
            },
            0,
        )
        .unwrap()
    }

    #[test]
    fn moving_between_clauses_keeps_all_uncommitted_text() {
        let mut session = sentence();
        for offset in [1, 1, 1, -1, -1, -1] {
            session.move_target(offset);
            let (before, active, after) = session.display();
            assert_eq!(format!("{before}{active}{after}"), "今日は晴れですね");
            assert_eq!(
                session
                    .clauses
                    .iter()
                    .map(|clause| clause.reading.as_str())
                    .collect::<String>(),
                "きょうははれですね"
            );
        }
        assert_eq!(session.active, 0);
    }

    #[test]
    fn resizing_preserves_reading_and_handles_unicode_and_boundaries() {
        let mut session = sentence();
        assert!(session.resize(-1));
        assert_eq!(session.clauses[0].reading, "きょう");
        assert_eq!(session.clauses[1].reading, "ははれ");
        assert!(session.resize(1));
        assert_eq!(session.clauses[0].reading, "きょうは");
        session.active = 2;
        assert!(!session.resize(1));
        session.clauses[2].reading = "𠮷あ".into();
        assert!(session.resize(-1));
        assert_eq!(session.clauses[2].reading, "𠮷");
        assert!(!session.resize(-1));
        assert!(session.resize(1));
        assert_eq!(session.clauses[2].reading, "𠮷あ");
    }
}
