use std::cmp::{max, min};

use crate::{
    engine::user_action::UserAction,
    extension::VKeyExt as _,
    tsf::factory::{TextServiceFactory, TextServiceFactory_Impl},
};

use super::{
    clauses::ClauseSession,
    client_action::{ClientAction, SetSelectionType, SetTextType},
    full_width::{to_fullwidth, to_halfwidth},
    input_mode::InputMode,
    ipc_service::Candidates,
    state::IMEState,
    text_util::{to_half_katakana, to_katakana},
    user_action::{Function, Navigation},
};
use windows::Win32::{
    Foundation::WPARAM,
    UI::{
        Input::KeyboardAndMouse::VK_CONTROL,
        TextServices::{ITfComposition, ITfCompositionSink_Impl, ITfContext},
    },
};

use anyhow::{Context, Result};

#[derive(Default, Clone, PartialEq, Debug)]
pub enum CompositionState {
    #[default]
    None,
    Composing,
    Previewing,
    Selecting,
}

#[derive(Default, Clone, Debug)]
pub struct Composition {
    pub preview: String, // text to be previewed
    pub suffix: String,  // text to be appended after preview
    pub raw_input: String,
    pub raw_hiragana: String,

    pub corresponding_count: i32, // corresponding count of the preview

    pub selection_index: i32,
    pub candidates: Candidates,

    pub state: CompositionState,
    pub tip_composition: Option<ITfComposition>,
    pub clause_session: Option<ClauseSession>,
}

fn normalize_kana_input(text: &str, symbol_input_style: &str) -> String {
    match symbol_input_style {
        "raw" => text.to_string(),
        _ => to_fullwidth(text, false),
    }
}

fn normalize_keyboard_layout_input(text: &str, keyboard_layout: &str) -> String {
    match keyboard_layout {
        "dvorak_qwerty" => text.chars().map(dvorak_to_qwerty).collect(),
        "colemak_qwerty" => text.chars().map(colemak_to_qwerty).collect(),
        _ => text.to_string(),
    }
}

fn colemak_to_qwerty(ch: char) -> char {
    let lower = ch.to_ascii_lowercase();
    let mapped = match lower {
        'f' => 'e',
        'p' => 'r',
        'g' => 't',
        'j' => 'y',
        'l' => 'u',
        'u' => 'i',
        'y' => 'o',
        ';' => 'p',
        'r' => 's',
        's' => 'd',
        't' => 'f',
        'd' => 'g',
        'n' => 'j',
        'e' => 'k',
        'i' => 'l',
        'o' => ';',
        'k' => 'n',
        _ => ch,
    };
    if ch.is_ascii_uppercase() {
        mapped.to_ascii_uppercase()
    } else {
        mapped
    }
}

fn dvorak_to_qwerty(ch: char) -> char {
    let lower = ch.to_ascii_lowercase();
    let mapped = match lower {
        '\'' => 'q',
        ',' => 'w',
        '.' => 'e',
        'p' => 'r',
        'y' => 't',
        'f' => 'y',
        'g' => 'u',
        'c' => 'i',
        'r' => 'o',
        'l' => 'p',
        '/' => '[',
        '=' => ']',
        'a' => 'a',
        'o' => 's',
        'e' => 'd',
        'u' => 'f',
        'i' => 'g',
        'd' => 'h',
        'h' => 'j',
        't' => 'k',
        'n' => 'l',
        's' => ';',
        '-' => '\'',
        ';' => 'z',
        'q' => 'x',
        'j' => 'c',
        'k' => 'v',
        'x' => 'b',
        'b' => 'n',
        'm' => 'm',
        'w' => ',',
        'v' => '.',
        'z' => '/',
        _ => ch,
    };

    if ch.is_ascii_uppercase() {
        mapped.to_ascii_uppercase()
    } else {
        mapped
    }
}

impl ITfCompositionSink_Impl for TextServiceFactory_Impl {
    #[macros::anyhow]
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        _pcomposition: Option<&ITfComposition>,
    ) -> Result<()> {
        // if user clicked outside the composition, the composition will be terminated
        tracing::debug!("OnCompositionTerminated");

        let actions = vec![ClientAction::EndComposition];
        self.handle_action(&actions, CompositionState::None)?;

        Ok(())
    }
}

impl TextServiceFactory {
    fn handle_clause_action(&self, actions: &[ClientAction]) -> Result<bool> {
        let [action] = actions else {
            return Ok(false);
        };
        let composition = self.borrow()?.borrow_composition()?.clone();
        let starting = matches!(
            action,
            ClientAction::MoveClause(_) | ClientAction::ResizeConversion(_)
        );
        let continuing = composition.clause_session.is_some()
            && matches!(
                action,
                ClientAction::SetSelection(_)
                    | ClientAction::RequestCandidates { .. }
                    | ClientAction::SetTextWithType(
                        SetTextType::Hiragana | SetTextType::Katakana | SetTextType::HalfKatakana
                    )
            );
        if !starting && !continuing {
            return Ok(false);
        }
        self.update_context(&composition.preview, &composition.suffix)?;
        let mut ipc = IMEState::get()?
            .ipc_service
            .clone()
            .context("ipc_service is None")?;
        let mut candidates = composition.candidates.clone();
        let mut session = if let Some(session) = composition.clause_session.clone() {
            session
        } else {
            if composition.state == CompositionState::Composing || candidates.texts.is_empty() {
                candidates = ipc.convert_text(composition.raw_hiragana.clone(), false)?;
            }
            if candidates.texts.is_empty() {
                return Ok(true);
            }
            ClauseSession::new(
                &candidates,
                (composition.selection_index.max(0) as usize).min(candidates.texts.len() - 1),
            )?
        };
        let mut selection = composition.selection_index.max(0) as usize;
        let mut query = false;
        let mut choose_first = false;
        let mut prediction = false;
        let mut resized = false;
        match action {
            ClientAction::MoveClause(offset) => {
                session.move_target(*offset);
                query = true;
            }
            ClientAction::ResizeConversion(offset) => {
                if session.resize(*offset) {
                    resized = true;
                    choose_first = true;
                }
                query = true;
            }
            ClientAction::RequestCandidates {
                prediction: requested,
            } => {
                query = true;
                choose_first = true;
                prediction = *requested;
            }
            ClientAction::SetSelection(requested) => {
                if candidates.texts.is_empty() {
                    query = true;
                    choose_first = true;
                } else {
                    selection = match requested {
                        SetSelectionType::Up => selection.saturating_sub(1),
                        SetSelectionType::Down => (selection + 1).min(candidates.texts.len() - 1),
                        SetSelectionType::Number(index) => (*index).max(0) as usize,
                    }
                    .min(candidates.texts.len() - 1);
                    session.clauses[session.active].text = candidates.texts[selection].clone();
                }
            }
            ClientAction::SetTextWithType(kind) => {
                let clause = &mut session.clauses[session.active];
                clause.pending = false;
                clause.text = match kind {
                    SetTextType::Hiragana => clause.reading.clone(),
                    SetTextType::Katakana => to_katakana(&clause.reading),
                    SetTextType::HalfKatakana => to_half_katakana(&clause.reading),
                    _ => unreachable!(),
                };
                candidates = Candidates::default();
                selection = 0;
            }
            _ => unreachable!(),
        }
        if query {
            let (before, _, _) = session.display();
            candidates = ipc.convert_clause(
                session.clauses[session.active].reading.clone(),
                &before,
                prediction,
            )?;
            if candidates.texts.is_empty() {
                ipc.set_candidates(&composition.candidates)?;
                ipc.set_selection(composition.selection_index)?;
                return Ok(true);
            }
            if session.clauses[session.active].pending && !prediction {
                let expanded = ClauseSession::new(&candidates, 0)?;
                session
                    .clauses
                    .splice(session.active..=session.active, expanded.clauses);
                candidates = ipc.convert_clause(
                    session.clauses[session.active].reading.clone(),
                    &before,
                    false,
                )?;
                choose_first = true;
            }
            let current = &session.clauses[session.active].text;
            selection = if choose_first {
                0
            } else {
                match candidates.texts.iter().position(|text| text == current) {
                    Some(index) => index,
                    None => {
                        candidates.texts.insert(0, current.clone());
                        candidates.sub_texts.insert(0, String::new());
                        candidates.corresponding_count.insert(
                            0,
                            session.clauses[session.active].reading.chars().count() as i32,
                        );
                        candidates.is_prediction.insert(0, false);
                        candidates.clauses.insert(0, vec![]);
                        0
                    }
                }
            };
            session.clauses[session.active].text = candidates.texts[selection].clone();
        }
        if resized {
            if let Some(next) = session.clauses.get(session.active + 1) {
                let before: String = session.clauses[..=session.active]
                    .iter()
                    .map(|clause| clause.text.as_str())
                    .collect();
                let next_candidates = ipc.convert_clause(next.reading.clone(), &before, false)?;
                session.clauses[session.active + 1].text = next_candidates.texts[0].clone();
                session.clauses[session.active + 1].pending = false;
            }
        }
        ipc.set_candidates(&candidates)?;
        ipc.set_selection(selection as i32)?;
        let (before, active, after) = session.display();
        self.set_clause_text(&before, &active, &after)?;
        {
            let service = self.borrow()?;
            let mut composition = service.borrow_mut_composition()?;
            composition.preview = format!("{before}{active}{after}");
            composition.suffix.clear();
            composition.corresponding_count = composition.raw_hiragana.chars().count() as i32;
            composition.candidates = candidates;
            composition.selection_index = selection as i32;
            composition.state = CompositionState::Previewing;
            composition.clause_session = Some(session);
        }
        self.update_pos()?;
        Ok(true)
    }

    #[tracing::instrument]
    pub fn process_key(
        &self,
        context: Option<&ITfContext>,
        wparam: WPARAM,
    ) -> Result<Option<(Vec<ClientAction>, CompositionState)>> {
        if context.is_none() {
            return Ok(None);
        };

        // check shortcut keys
        if VK_CONTROL.is_pressed() {
            return Ok(None);
        }

        #[allow(clippy::let_and_return)]
        let (composition, mode) = {
            let text_service = self.borrow()?;
            let composition = text_service.borrow_composition()?.clone();
            let mode = IMEState::get()?.input_mode.clone();
            (composition, mode)
        };

        key_actions(
            composition,
            mode,
            UserAction::try_from(wparam.0)?,
            shared::AppConfig::read().conversion,
        )
    }

    #[tracing::instrument]
    pub fn handle_key(&self, context: Option<&ITfContext>, wparam: WPARAM) -> Result<bool> {
        if let Some(context) = context {
            self.borrow_mut()?.context = Some(context.clone());
        } else {
            return Ok(false);
        };

        if let Some((actions, transition)) = self.process_key(context, wparam)? {
            self.handle_action(&actions, transition)?;
        } else {
            return Ok(false);
        }

        Ok(true)
    }

    #[tracing::instrument]
    pub fn handle_action(
        &self,
        actions: &[ClientAction],
        transition: CompositionState,
    ) -> Result<()> {
        if self.handle_clause_action(actions)? {
            return Ok(());
        }
        #[allow(clippy::let_and_return)]
        let (composition, mode) = {
            let text_service = self.borrow()?;
            let composition = text_service.borrow_composition()?.clone();
            let mode = IMEState::get()?.input_mode.clone();
            (composition, mode)
        };

        let mut preview = composition.preview.clone();
        let mut suffix = composition.suffix.clone();
        let mut raw_input = composition.raw_input.clone();
        let mut raw_hiragana = composition.raw_hiragana.clone();
        let mut corresponding_count = composition.corresponding_count;
        let mut candidates = composition.candidates.clone();
        let mut selection_index = composition.selection_index;
        let app_config = shared::AppConfig::read();
        let symbol_input_style = app_config.conversion.symbol_input_style;
        let keyboard_layout = app_config.conversion.keyboard_layout;
        let mut ipc_service = IMEState::get()?
            .ipc_service
            .clone()
            .context("ipc_service is None")?;
        let mut transition = transition;

        self.update_context(&preview, &suffix)?;

        for action in actions {
            match action {
                ClientAction::StartComposition => {
                    self.start_composition()?;
                    self.update_pos()?;
                    ipc_service.show_window()?;
                }
                ClientAction::CommitCandidate => {
                    let reading = raw_hiragana
                        .chars()
                        .take(corresponding_count.max(0) as usize)
                        .collect::<String>();
                    if !reading.is_empty() && !preview.is_empty() {
                        ipc_service.commit_candidate(reading, preview.clone())?;
                    }
                }
                ClientAction::EndComposition => {
                    self.end_composition()?;
                    selection_index = 0;
                    corresponding_count = 0;
                    preview.clear();
                    suffix.clear();
                    raw_input.clear();
                    raw_hiragana.clear();
                    ipc_service.hide_window()?;
                    ipc_service.set_candidates(&Candidates::default())?;
                    ipc_service.clear_text()?;
                }
                ClientAction::AppendText(text) => {
                    let text = normalize_keyboard_layout_input(text, &keyboard_layout);
                    raw_input.push_str(&text);

                    let text = match mode {
                        InputMode::Kana => normalize_kana_input(&text, &symbol_input_style),
                        InputMode::Latin => text.to_string(),
                    };

                    candidates = ipc_service.append_text(text.clone())?;
                    selection_index = candidate_for_key(&candidates, 0, false, true).unwrap_or(0);
                    let hiragana = candidates.hiragana.clone();
                    let (text, sub_text) =
                        if !app_config.conversion.live_conversion || candidates.is_latin_input() {
                            corresponding_count = hiragana.chars().count() as i32;
                            (hiragana.clone(), String::new())
                        } else {
                            corresponding_count =
                                candidates.corresponding_count[selection_index as usize];
                            (
                                candidates.texts[selection_index as usize].clone(),
                                candidates.sub_texts[selection_index as usize].clone(),
                            )
                        };
                    raw_input = candidates.raw_input.clone();

                    preview = text.clone();
                    suffix = sub_text.clone();
                    raw_hiragana = hiragana.clone();

                    self.set_text(&text, &sub_text)?;
                    ipc_service.schedule_prediction(hiragana)?;
                    ipc_service.set_selection(-1)?;
                }
                ClientAction::RemoveText => {
                    candidates = ipc_service.remove_text()?;
                    selection_index = candidate_for_key(&candidates, 0, false, true).unwrap_or(0);
                    let empty = "".to_string();
                    let text = candidates
                        .texts
                        .get(selection_index as usize)
                        .cloned()
                        .unwrap_or(empty.clone());
                    let sub_text = candidates
                        .sub_texts
                        .get(selection_index as usize)
                        .cloned()
                        .unwrap_or(empty.clone());
                    let hiragana = candidates.hiragana.clone();
                    corresponding_count = candidates
                        .corresponding_count
                        .get(selection_index as usize)
                        .cloned()
                        .unwrap_or(0);

                    let (text, sub_text) =
                        if !app_config.conversion.live_conversion || candidates.is_latin_input() {
                            corresponding_count = hiragana.chars().count() as i32;
                            (hiragana.clone(), String::new())
                        } else {
                            (text, sub_text)
                        };

                    raw_input = candidates.raw_input.clone();
                    preview = text.clone();
                    suffix = sub_text.clone();
                    raw_hiragana = hiragana.clone();

                    self.set_text(&text, &sub_text)?;
                    ipc_service.schedule_prediction(hiragana)?;
                    ipc_service.set_selection(-1)?;
                }
                ClientAction::MoveCursor(_offset) => {
                    // TODO: I'll use azookey-kkc's composingText
                    // self.set_cursor(offset)?;
                }
                ClientAction::MoveClause(_) | ClientAction::ResizeConversion(_) => {
                    unreachable!("Clause navigation handled above")
                }
                ClientAction::SetIMEMode(mode) => {
                    self.start_composition()?;
                    self.update_pos()?;
                    self.end_composition()?;

                    {
                        let mut ime_state = IMEState::get()?;
                        ime_state.input_mode = mode.clone();
                    } // release IME_STATE mutex before update_lang_bar() calls GetIcon()

                    // update the language bar
                    self.update_lang_bar()?;

                    let mode = match mode {
                        InputMode::Latin => "A",
                        InputMode::Kana => "あ",
                    };

                    ipc_service.set_input_mode(mode)?;

                    selection_index = 0;
                    corresponding_count = 0;
                    preview.clear();
                    suffix.clear();
                    raw_input.clear();
                    raw_hiragana.clear();
                    ipc_service.clear_text()?;
                }
                ClientAction::RequestCandidates { prediction } => {
                    candidates = ipc_service.convert_text(raw_hiragana.clone(), *prediction)?;
                    ipc_service.set_candidates(&candidates)?;
                    if candidates.texts.is_empty() {
                        transition = composition.state.clone();
                        continue;
                    }
                    selection_index = 0;
                    corresponding_count = candidates.corresponding_count[0];
                    preview = candidates.texts[0].clone();
                    suffix = candidates.sub_texts[0].clone();
                    self.set_text(&preview, &suffix)?;
                    ipc_service.set_selection(0)?;
                }
                ClientAction::SetSelection(selection) => {
                    let texts = candidates.texts.clone();
                    let sub_texts = candidates.sub_texts.clone();

                    if texts.is_empty() {
                        continue;
                    }

                    selection_index = match selection {
                        SetSelectionType::Up => max(0, selection_index - 1),
                        SetSelectionType::Down => min(texts.len() as i32 - 1, selection_index + 1),
                        SetSelectionType::Number(number) => {
                            min(max(0, *number), texts.len() as i32 - 1)
                        }
                    };

                    ipc_service.set_selection(selection_index)?;
                    let text = texts[selection_index as usize].clone();
                    let sub_text = sub_texts[selection_index as usize].clone();
                    let hiragana = candidates.hiragana.clone();
                    corresponding_count = candidates.corresponding_count[selection_index as usize];

                    preview = text.clone();
                    suffix = sub_text.clone();
                    raw_hiragana = hiragana.clone();

                    self.set_text(&text, &sub_text)?;
                }
                ClientAction::ShrinkText(text) => {
                    // shrink text
                    let text = normalize_keyboard_layout_input(text, &keyboard_layout);
                    ipc_service.shrink_text(corresponding_count)?;
                    let text = match mode {
                        InputMode::Kana => normalize_kana_input(&text, &symbol_input_style),
                        InputMode::Latin => text.to_string(),
                    };
                    candidates = ipc_service.append_text(text)?;
                    selection_index = candidate_for_key(&candidates, 0, false, true).unwrap_or(0);
                    let hiragana = candidates.hiragana.clone();
                    let (text, sub_text) =
                        if !app_config.conversion.live_conversion || candidates.is_latin_input() {
                            corresponding_count = hiragana.chars().count() as i32;
                            (hiragana.clone(), String::new())
                        } else {
                            corresponding_count =
                                candidates.corresponding_count[selection_index as usize];
                            (
                                candidates.texts[selection_index as usize].clone(),
                                candidates.sub_texts[selection_index as usize].clone(),
                            )
                        };
                    raw_input = candidates.raw_input.clone();
                    self.shift_start(&preview, &text)?;
                    self.set_text(&text, &sub_text)?;
                    preview = text.clone();
                    suffix = sub_text.clone();
                    raw_hiragana = hiragana.clone();

                    ipc_service.schedule_prediction(hiragana)?;
                    ipc_service.set_selection(-1)?;
                    self.update_pos()?;

                    transition = CompositionState::Composing;
                }
                ClientAction::SetTextWithType(set_type) => {
                    let text = match set_type {
                        SetTextType::Hiragana => raw_hiragana.clone(),
                        SetTextType::Katakana => to_katakana(&raw_hiragana),
                        SetTextType::HalfKatakana => to_half_katakana(&raw_hiragana),
                        SetTextType::FullLatin => to_fullwidth(&raw_input, true),
                        SetTextType::HalfLatin => to_halfwidth(&raw_input),
                    };

                    self.set_text(&text, "")?;
                    preview = text;
                    suffix.clear();
                    corresponding_count = raw_hiragana.chars().count() as i32;
                }
            }
        }

        let text_service = self.borrow()?;
        let mut composition = text_service.borrow_mut_composition()?;

        composition.preview = preview.clone();
        composition.state = transition;
        composition.selection_index = selection_index;
        composition.raw_input = raw_input.clone();
        composition.raw_hiragana = raw_hiragana.clone();
        composition.candidates = candidates;
        composition.suffix = suffix.clone();
        composition.corresponding_count = corresponding_count;
        composition.clause_session = None;

        Ok(())
    }
}

fn key_actions(
    composition: Composition,
    mode: InputMode,
    mut action: UserAction,
    conversion_config: shared::ConversionConfig,
) -> Result<Option<(Vec<ClientAction>, CompositionState)>> {
    if matches!(action, UserAction::Muhenkan) {
        if conversion_config.muhenkan_action == "latin" {
            action = UserAction::SetInputMode(InputMode::Latin);
        } else if composition.state == CompositionState::None {
            return Ok(Some((vec![], CompositionState::None)));
        } else {
            let (text, reading) = composition
                .clause_session
                .as_ref()
                .map(|session| {
                    let clause = &session.clauses[session.active];
                    (clause.text.as_str(), clause.reading.as_str())
                })
                .unwrap_or((&composition.preview, &composition.raw_hiragana));
            let set_type = next_kana_type(text, reading);
            return Ok(Some((
                vec![ClientAction::SetTextWithType(set_type)],
                CompositionState::Previewing,
            )));
        }
    }
    if let UserAction::SetInputMode(target) = action {
        // Repeating the Hiragana key must not switch back to Latin or commit the composition.
        if target == mode {
            return Ok(Some((vec![], composition.state)));
        }
        let mut actions = Vec::new();
        if composition.state != CompositionState::None {
            actions.extend([ClientAction::CommitCandidate, ClientAction::EndComposition]);
        }
        actions.push(ClientAction::SetIMEMode(target));
        return Ok(Some((actions, CompositionState::None)));
    }
    let candidate_number_selection = conversion_config.candidate_number_selection;

    let (transition, actions) = match composition.state {
        CompositionState::None => match action {
            UserAction::Input(char) if mode == InputMode::Kana => (
                CompositionState::Composing,
                vec![
                    ClientAction::StartComposition,
                    ClientAction::AppendText(char.to_string()),
                ],
            ),
            UserAction::Number(number) if mode == InputMode::Kana => (
                CompositionState::Composing,
                vec![
                    ClientAction::StartComposition,
                    ClientAction::AppendText(number.to_string()),
                ],
            ),
            UserAction::ToggleInputMode => (
                CompositionState::None,
                vec![match mode {
                    InputMode::Kana => ClientAction::SetIMEMode(InputMode::Latin),
                    InputMode::Latin => ClientAction::SetIMEMode(InputMode::Kana),
                }],
            ),
            _ => {
                return Ok(None);
            }
        },
        CompositionState::Composing => match action {
            UserAction::Input(char) => (
                CompositionState::Composing,
                vec![ClientAction::AppendText(char.to_string())],
            ),
            UserAction::Number(number) => (
                CompositionState::Composing,
                vec![ClientAction::AppendText(number.to_string())],
            ),
            UserAction::Backspace => {
                if composition.preview.chars().count() == 1 {
                    (
                        CompositionState::None,
                        vec![ClientAction::RemoveText, ClientAction::EndComposition],
                    )
                } else {
                    (CompositionState::Composing, vec![ClientAction::RemoveText])
                }
            }
            UserAction::Enter => {
                if composition.suffix.is_empty() {
                    (
                        CompositionState::None,
                        vec![ClientAction::CommitCandidate, ClientAction::EndComposition],
                    )
                } else {
                    (
                        CompositionState::Composing,
                        vec![
                            ClientAction::CommitCandidate,
                            ClientAction::ShrinkText("".to_string()),
                        ],
                    )
                }
            }
            UserAction::Escape => (
                CompositionState::None,
                vec![ClientAction::RemoveText, ClientAction::EndComposition],
            ),
            UserAction::Navigation(direction) => match direction {
                Navigation::Right | Navigation::Left => (
                    CompositionState::Previewing,
                    vec![ClientAction::MoveClause(
                        if matches!(direction, Navigation::Right) {
                            1
                        } else {
                            -1
                        },
                    )],
                ),
                Navigation::Up => (
                    CompositionState::Previewing,
                    vec![ClientAction::RequestCandidates { prediction: true }],
                ),
                Navigation::Down => (
                    CompositionState::Previewing,
                    vec![ClientAction::RequestCandidates { prediction: true }],
                ),
            },
            UserAction::ShiftedNavigation(direction) => match direction {
                Navigation::Right => (
                    CompositionState::Previewing,
                    vec![ClientAction::ResizeConversion(1)],
                ),
                Navigation::Left => (
                    CompositionState::Previewing,
                    vec![ClientAction::ResizeConversion(-1)],
                ),
                _ => return Ok(None),
            },
            UserAction::ToggleInputMode => (
                CompositionState::None,
                vec![
                    ClientAction::EndComposition,
                    ClientAction::SetIMEMode(InputMode::Latin),
                ],
            ),
            UserAction::Space | UserAction::Tab => {
                let prediction = matches!(action, UserAction::Tab);
                (
                    CompositionState::Previewing,
                    vec![ClientAction::RequestCandidates { prediction }],
                )
            }
            UserAction::Function(key) => match key {
                Function::Six => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::Hiragana)],
                ),
                Function::Seven => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::Katakana)],
                ),
                Function::Eight => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::HalfKatakana)],
                ),
                Function::Nine => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::FullLatin)],
                ),
                Function::Ten => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::HalfLatin)],
                ),
            },
            _ => {
                return Ok(None);
            }
        },
        CompositionState::Previewing => match action {
            UserAction::Input(char) => (
                CompositionState::Composing,
                vec![
                    ClientAction::CommitCandidate,
                    ClientAction::ShrinkText(char.to_string()),
                ],
            ),
            UserAction::Number(number) if candidate_number_selection => (
                CompositionState::Previewing,
                vec![ClientAction::SetSelection(SetSelectionType::Number(
                    candidate_number_to_index(number),
                ))],
            ),
            UserAction::Number(number) => (
                CompositionState::Composing,
                vec![
                    ClientAction::CommitCandidate,
                    ClientAction::ShrinkText(number.to_string()),
                ],
            ),
            UserAction::Backspace => {
                if composition.preview.chars().count() == 1 {
                    (
                        CompositionState::None,
                        vec![ClientAction::RemoveText, ClientAction::EndComposition],
                    )
                } else {
                    (CompositionState::Composing, vec![ClientAction::RemoveText])
                }
            }
            UserAction::Enter => {
                if composition.suffix.is_empty() {
                    (
                        CompositionState::None,
                        vec![ClientAction::CommitCandidate, ClientAction::EndComposition],
                    )
                } else {
                    (
                        CompositionState::Composing,
                        vec![
                            ClientAction::CommitCandidate,
                            ClientAction::ShrinkText("".to_string()),
                        ],
                    )
                }
            }
            UserAction::Escape => (
                CompositionState::None,
                vec![ClientAction::RemoveText, ClientAction::EndComposition],
            ),
            UserAction::Navigation(direction) => match direction {
                Navigation::Right | Navigation::Left => (
                    CompositionState::Previewing,
                    vec![ClientAction::MoveClause(
                        if matches!(direction, Navigation::Right) {
                            1
                        } else {
                            -1
                        },
                    )],
                ),
                Navigation::Up => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetSelection(SetSelectionType::Up)],
                ),
                Navigation::Down => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetSelection(SetSelectionType::Down)],
                ),
            },
            UserAction::ShiftedNavigation(direction) => match direction {
                Navigation::Right => (
                    CompositionState::Previewing,
                    vec![ClientAction::ResizeConversion(1)],
                ),
                Navigation::Left => (
                    CompositionState::Previewing,
                    vec![ClientAction::ResizeConversion(-1)],
                ),
                _ => return Ok(None),
            },
            UserAction::ToggleInputMode => (
                CompositionState::None,
                vec![
                    ClientAction::EndComposition,
                    ClientAction::SetIMEMode(InputMode::Latin),
                ],
            ),
            UserAction::Space | UserAction::Tab => {
                let prediction = matches!(action, UserAction::Tab);
                let Some(index) = candidate_for_key(
                    &composition.candidates,
                    composition.selection_index,
                    prediction,
                    false,
                ) else {
                    return Ok(Some((
                        vec![ClientAction::RequestCandidates { prediction }],
                        CompositionState::Previewing,
                    )));
                };
                (
                    CompositionState::Previewing,
                    vec![ClientAction::SetSelection(SetSelectionType::Number(index))],
                )
            }
            UserAction::Function(key) => match key {
                Function::Six => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::Hiragana)],
                ),
                Function::Seven => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::Katakana)],
                ),
                Function::Eight => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::HalfKatakana)],
                ),
                Function::Nine => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::FullLatin)],
                ),
                Function::Ten => (
                    CompositionState::Previewing,
                    vec![ClientAction::SetTextWithType(SetTextType::HalfLatin)],
                ),
            },
            _ => {
                return Ok(None);
            }
        },
        _ => {
            return Ok(None);
        }
    };

    Ok(Some((actions, transition)))
}

fn next_kana_type(text: &str, reading: &str) -> SetTextType {
    if text == to_katakana(reading) && text != reading {
        SetTextType::HalfKatakana
    } else if text == to_half_katakana(reading) && text != reading {
        SetTextType::Hiragana
    } else {
        SetTextType::Katakana
    }
}

fn candidate_for_key(
    candidates: &Candidates,
    current: i32,
    prediction: bool,
    first: bool,
) -> Option<i32> {
    let eligible = candidates
        .is_prediction
        .iter()
        .enumerate()
        .filter(|(_, is_prediction)| **is_prediction == prediction)
        .map(|(index, _)| index as i32)
        .collect::<Vec<_>>();
    if !first && eligible.contains(&current) {
        eligible
            .iter()
            .copied()
            .find(|index| *index > current)
            .or_else(|| eligible.first().copied())
    } else {
        eligible.first().copied()
    }
}

fn candidate_number_to_index(number: i8) -> i32 {
    match number {
        0 => 9,
        1..=9 => number as i32 - 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_keys_never_commit_partial_or_full_conversions() {
        for state in [CompositionState::Composing, CompositionState::Previewing] {
            for suffix in ["", "ですね"] {
                for (action, expected) in [
                    (
                        UserAction::Navigation(Navigation::Left),
                        ClientAction::MoveClause(-1),
                    ),
                    (
                        UserAction::Navigation(Navigation::Right),
                        ClientAction::MoveClause(1),
                    ),
                    (
                        UserAction::ShiftedNavigation(Navigation::Left),
                        ClientAction::ResizeConversion(-1),
                    ),
                    (
                        UserAction::ShiftedNavigation(Navigation::Right),
                        ClientAction::ResizeConversion(1),
                    ),
                ] {
                    let composition = Composition {
                        state: state.clone(),
                        preview: "今日は晴れ".into(),
                        suffix: suffix.into(),
                        ..Composition::default()
                    };
                    let (actions, transition) = key_actions(
                        composition,
                        InputMode::Kana,
                        action,
                        shared::ConversionConfig::default(),
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(transition, CompositionState::Previewing);
                    assert_eq!(actions, vec![expected]);
                    assert!(!actions.contains(&ClientAction::CommitCandidate));
                    assert!(!actions.contains(&ClientAction::EndComposition));
                }
            }
        }
    }

    #[test]
    fn muhenkan_cycles_kana_and_can_switch_to_latin() {
        for (text, expected) in [
            ("漢字", SetTextType::Katakana),
            ("カンジ", SetTextType::HalfKatakana),
            ("ｶﾝｼﾞ", SetTextType::Hiragana),
        ] {
            let composition = Composition {
                state: CompositionState::Previewing,
                preview: text.into(),
                raw_hiragana: "かんじ".into(),
                ..Composition::default()
            };
            let (actions, transition) = key_actions(
                composition,
                InputMode::Kana,
                UserAction::Muhenkan,
                shared::ConversionConfig::default(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(actions, vec![ClientAction::SetTextWithType(expected)]);
            assert_eq!(transition, CompositionState::Previewing);
        }
        let config = shared::ConversionConfig {
            muhenkan_action: "latin".into(),
            ..shared::ConversionConfig::default()
        };
        let composition = Composition {
            state: CompositionState::Previewing,
            ..Composition::default()
        };
        let (actions, transition) =
            key_actions(composition, InputMode::Kana, UserAction::Muhenkan, config)
                .unwrap()
                .unwrap();
        assert_eq!(
            actions,
            vec![
                ClientAction::CommitCandidate,
                ClientAction::EndComposition,
                ClientAction::SetIMEMode(InputMode::Latin)
            ]
        );
        assert_eq!(transition, CompositionState::None);
    }

    #[test]
    fn space_and_tab_use_separate_candidate_lists() {
        let candidates = Candidates {
            texts: ["漢字", "感じ", "漢字変換", "漢字辞典"]
                .map(String::from)
                .to_vec(),
            is_prediction: vec![false, false, true, true],
            ..Candidates::default()
        };
        assert_eq!(candidate_for_key(&candidates, 0, false, true), Some(0));
        assert_eq!(candidate_for_key(&candidates, 0, false, false), Some(1));
        assert_eq!(candidate_for_key(&candidates, 1, false, false), Some(0));
        assert_eq!(candidate_for_key(&candidates, 0, true, true), Some(2));
        assert_eq!(candidate_for_key(&candidates, 2, true, false), Some(3));
        assert_eq!(candidate_for_key(&candidates, 3, true, false), Some(2));
        assert_eq!(candidate_for_key(&candidates, 3, false, false), Some(0));
    }

    #[test]
    fn live_preview_skips_predictions_and_tab_does_nothing_without_predictions() {
        let candidates = Candidates {
            is_prediction: vec![true, false],
            ..Candidates::default()
        };
        assert_eq!(candidate_for_key(&candidates, 0, false, true), Some(1));
        let candidates = Candidates {
            is_prediction: vec![false, false],
            ..Candidates::default()
        };
        assert_eq!(candidate_for_key(&candidates, 0, true, true), None);
    }

    #[test]
    fn maps_number_keys_to_candidate_indexes() {
        assert_eq!(candidate_number_to_index(1), 0);
        assert_eq!(candidate_number_to_index(9), 8);
        assert_eq!(candidate_number_to_index(0), 9);
    }

    #[test]
    fn capitalized_english_remains_literal_during_live_conversion() {
        for (raw, literal) in [
            ("Windows", true),
            ("windows", false),
            ("kanji", false),
            ("Windowsかな", false),
        ] {
            let candidates = Candidates {
                raw_input: raw.into(),
                ..Candidates::default()
            };
            assert_eq!(candidates.is_latin_input(), literal);
        }
    }

    #[test]
    fn normalizes_kana_symbols_from_config() {
        assert_eq!(normalize_kana_input(",", "japanese"), "、");
        assert_eq!(normalize_kana_input(".", "japanese"), "。");
        assert_eq!(normalize_kana_input(",", "raw"), ",");
    }

    #[test]
    fn maps_dvorak_output_to_qwerty_input() {
        assert_eq!(
            normalize_keyboard_layout_input("dhtns", "dvorak_qwerty"),
            "hjkl;"
        );
        assert_eq!(
            normalize_keyboard_layout_input("',.py", "dvorak_qwerty"),
            "qwert"
        );
        assert_eq!(normalize_keyboard_layout_input("abc", "system"), "abc");
    }

    #[test]
    fn maps_colemak_output_to_qwerty_input() {
        // Colemak f→e, p→r, g→t, j→y, l→u, u→i
        assert_eq!(
            normalize_keyboard_layout_input("fpgjlu", "colemak_qwerty"),
            "ertyui"
        );
        // Colemak n→j, e→k, i→l
        assert_eq!(
            normalize_keyboard_layout_input("nei", "colemak_qwerty"),
            "jkl"
        );
        assert_eq!(
            normalize_keyboard_layout_input("abc", "colemak_qwerty"),
            "abc"
        );
    }
}
