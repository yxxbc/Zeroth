//! Full-screen workspace focus and conversation switching.
use super::*;

#[derive(Debug)]
pub(super) enum TabFocus {
    Main,
    Agent(String),
}

#[derive(Debug, Default)]
pub(super) struct WorkspaceDraft {
    input: TextArea,
    shell_mode: bool,
    pasted: Vec<(String, String)>,
    pub(super) images: Vec<(String, PathBuf)>,
}

impl App {
    /// Subagent tabs in roster order, including settled agents still listed.
    pub(crate) fn sessions(&self) -> Vec<&AgentRun> {
        self.visible_agents()
    }

    /// Index of the focused tab, with main at zero. A vanished tab falls
    /// back to the conversation on screen, not to its former neighbour.
    #[must_use]
    pub fn session_bar_focused(&self) -> Option<usize> {
        Some(match self.session_bar.as_ref()? {
            TabFocus::Main => 0,
            TabFocus::Agent(id) => self
                .session_index(Some(id))
                .unwrap_or_else(|| self.viewed_tab_index()),
        })
    }

    fn session_index(&self, id: Option<&str>) -> Option<usize> {
        match id {
            None => Some(0),
            Some(id) => self
                .sessions()
                .iter()
                .position(|run| run.id == id)
                .map(|index| index + 1),
        }
    }

    pub(crate) fn viewed_tab_index(&self) -> usize {
        self.session_index(self.agent_view.as_deref()).unwrap_or(0)
    }

    /// Whether workspace tab controls may act on the current frame.
    #[must_use]
    pub fn session_tabs_available(&self) -> bool {
        self.fullscreen
            && self.view == View::Conversation
            && !self.modal_open()
            && self.config_review.is_none()
            && self.history_search.is_none()
            && self.command_menu.is_none()
            && self.file_search.is_none()
            && !self.skill_band_active()
            && !self.shortcuts_open
            && self.background_view.is_none()
            && self.model_picker.is_none()
            && self.key_onboarding.is_none()
            && self.settings_picker.is_none()
            && self.mascot_picker.is_none()
            && self.spinner_picker.is_none()
            && self.theme_picker.is_none()
            && self.donate_picker.is_none()
            && self.export_picker.is_none()
            && self.context_usage.is_none()
            && self.usage_page.is_none()
            && self.secrets_page.is_none()
            && self.skills_menu.is_none()
            && self.hooks_menu.is_none()
    }

    pub(super) fn focus_session_bar(&mut self) {
        self.session_bar = Some(
            self.agent_view
                .clone()
                .map_or(TabFocus::Main, TabFocus::Agent),
        );
    }

    fn set_tab_focus(&mut self, index: usize) {
        self.session_bar = Some(
            index
                .checked_sub(1)
                .and_then(|index| self.sessions().get(index).map(|run| run.id.clone()))
                .map_or(TabFocus::Main, TabFocus::Agent),
        );
    }

    pub(super) fn on_key_session_bar(&mut self, key: KeyEvent) -> Option<Action> {
        let index = self.session_bar_focused()?;
        match (key.code, key.modifiers) {
            (KeyCode::Left, KeyModifiers::NONE) => self.set_tab_focus(index.saturating_sub(1)),
            (KeyCode::Right, KeyModifiers::NONE) => {
                self.set_tab_focus((index + 1).min(self.sessions().len()))
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                let id = index
                    .checked_sub(1)
                    .and_then(|i| self.sessions().get(i).map(|run| run.id.clone()));
                return Some(self.select_session_tab(id.as_deref()));
            }
            (KeyCode::Up | KeyCode::Esc, KeyModifiers::NONE)
            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.focus_workspace_composer(),
            (KeyCode::Down, KeyModifiers::NONE) => {}
            _ => {
                self.session_bar = None;
                return None;
            }
        }
        Some(Action::None)
    }

    /// Select a tab by identity, shared by Enter and the last-painted click
    /// map. An expired tab or a covered conversation is a no-op.
    pub fn select_session_tab(&mut self, id: Option<&str>) -> Action {
        if !self.session_tabs_available() || self.session_index(id).is_none() {
            return Action::None;
        }
        self.session_bar = Some(id.map_or(TabFocus::Main, |id| TabFocus::Agent(id.to_string())));
        self.backtrack = Backtrack::default();
        if self.agent_view.as_deref() == id {
            return Action::None;
        }
        if let Some(id) = id {
            self.open_agent_view(id);
            Action::ViewAgent(id.to_string())
        } else {
            self.close_agent_view();
            Action::LeaveAgentView
        }
    }

    /// Give the workspace composer focus (e.g. click or paste into it).
    pub fn focus_workspace_composer(&mut self) {
        self.session_bar = None;
    }

    pub(super) fn discard_workspace_draft(&mut self, id: &str) {
        if let Some(draft) = self.workspace_drafts.remove(&Some(id.to_string())) {
            self.discarded_images
                .extend(draft.images.into_iter().map(|(_, path)| path));
        }
    }

    pub(super) fn swap_workspace_draft(&mut self, to: Option<&str>) {
        let incoming = self
            .workspace_drafts
            .remove(&to.map(str::to_string))
            .unwrap_or_default();
        let outgoing = WorkspaceDraft {
            input: std::mem::replace(&mut self.input, incoming.input),
            shell_mode: std::mem::replace(&mut self.shell_mode, incoming.shell_mode),
            pasted: std::mem::replace(&mut self.pasted, incoming.pasted),
            images: std::mem::replace(&mut self.images, incoming.images),
        };
        if !outgoing.input.is_empty() || outgoing.shell_mode {
            self.workspace_drafts
                .insert(self.agent_view.clone(), outgoing);
        }
        self.input_history.reset_navigation();
    }
}
