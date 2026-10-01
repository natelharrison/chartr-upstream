use super::*;
use crate::conversations::Event;
use chartr_conversations::{NativeSession, Observation, Provider, Status};
use std::sync::Mutex;

// One lease across every window prevents two click-initiated writers while
// their new terminals are still waiting to report their native identities.
static RESUMING: Mutex<Vec<(Provider, NativeSession)>> = Mutex::new(Vec::new());

pub(super) struct ResumeLease {
    provider: Provider,
    native: NativeSession,
}

impl ResumeLease {
    fn reserve(provider: Provider, native: NativeSession) -> Result<Self, String> {
        let mut held = RESUMING.lock().expect("locking resume identities");
        if held.iter().any(|(kind, session)| *kind == provider && session == &native) {
            return Err("This conversation is already live in another terminal.".into());
        }
        held.push((provider, native.clone()));
        Ok(Self { provider, native })
    }
}

impl Drop for ResumeLease {
    fn drop(&mut self) {
        let mut held = RESUMING.lock().expect("releasing resume identity");
        held.retain(|(kind, session)| *kind != self.provider || session != &self.native);
    }
}

impl WorkspaceWindow {
    pub(super) fn prune_resume_owners(&mut self, cx: &App) {
        self.resume_owners.retain(|(_, owner, item, pending)| {
            *pending
                || self.spaces.iter().any(|candidate| {
                    candidate.entity_id() == *owner
                        && candidate.read(cx).item(*item).is_some_and(|entry| {
                            entry
                                .as_session()
                                .is_some_and(|session| session.session.ended().is_none())
                        })
                })
        });
    }

    pub(super) fn resume_ended(&mut self, id: crate::workspace::ItemId, cx: &mut Context<Self>) {
        let result = (|| {
            let space = self.active.as_ref().ok_or("The space is unavailable.")?.clone();
            let recovery = match space.read(cx).item(id) {
                Some(crate::item::Item::Ended(recovery)) => recovery.clone(),
                _ => return Err("This tab is no longer ended.".into()),
            };
            if !matches!(self.backend, Backend::Ready) {
                return Err("Wait for the terminal service to connect.".into());
            }
            let provider = recovery
                .agent
                .as_deref()
                .and_then(Provider::detect)
                .ok_or("No agent was recorded for this tab.")?;
            let native = recovery.native.as_ref();
            resume_preflight(native, true, false)?;
            let native = native.unwrap();
            if !matches!(provider, Provider::Pi | Provider::Claude | Provider::Codex) {
                return Err("This agent does not support resume here.".into());
            }
            let rows = self.conversations.read(cx);
            let row = rows
                .rows()
                .iter()
                .find(|row| row.provider == provider && row.native.as_ref() == Some(native));
            let path = chartr_conversations::ProviderPaths::from_environment()
                .session_log(provider, native)
                .map_err(|_| "The transcript is missing.".to_owned())?;
            let argument = if provider == Provider::Pi {
                path.to_str().ok_or("The session path is not valid UTF-8.")?
            } else {
                native.id.as_str()
            };
            let live = self.spaces.iter().any(|space| {
                let space = space.read(cx);
                space.owned_session_ids().iter().any(|backend| {
                    space.session_item(backend).is_some_and(|item| {
                        let info = &item.session.info;
                        item.session.ended().is_none()
                            && info.agent.as_deref().and_then(Provider::detect) == Some(provider)
                            && info.agent_session.as_ref().is_some_and(|identity| {
                                NativeSession::from_identity(
                                    provider,
                                    &identity.kind,
                                    &identity.value,
                                )
                                .as_ref()
                                    == Some(native)
                            })
                    })
                })
            });
            self.prune_resume_owners(cx);
            resume_preflight(
                Some(native),
                path.is_file(),
                live || row.is_some_and(|row| row.runtime.is_some()),
            )?;
            let service = self
                .catalog
                .services
                .get::<chartr_plugin::services::ResumeAgents>(
                    chartr_plugin::services::AGENT_SERVICE,
                )
                .ok_or("Enable Agent and register an agent first.")?;
            let input = service.prepare(provider.slug(), argument, cx)?;
            let lease = ResumeLease::reserve(provider, native.clone())?;
            let owner = space.entity_id();
            self.resume_owners.push((lease, owner, id, true));
            let task = space.update(cx, |space, cx| space.reopen_ended(id, Some(input), cx));
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    if result.is_err() {
                        this.resume_owners
                            .retain(|(_, candidate, item, _)| *candidate != owner || *item != id);
                    } else if let Some((_, _, _, pending)) = this
                        .resume_owners
                        .iter_mut()
                        .find(|(_, candidate, item, _)| *candidate == owner && *item == id)
                    {
                        *pending = false;
                    }
                    this.prune_resume_owners(cx);
                    if let Err(error) = result {
                        this.resume_error = Some((owner, id, error.clone()));
                        this.problem = Some(error);
                    } else {
                        this.resume_error = None;
                    }
                    cx.notify();
                });
            })
            .detach();
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            if let Some(space) = &self.active {
                self.resume_error = Some((space.entity_id(), id, error.clone()));
            }
            self.problem = Some(error);
            cx.notify();
        } else {
            self.resume_error = None;
        }
    }

    pub(super) fn agent_choices(&self, cx: &App) -> chrome::AgentChoices {
        chrome::AgentChoices {
            names: self.conversations.read(cx).registered_agent_names(cx).unwrap_or_default(),
            last_used: self.last_agent.clone(),
        }
    }

    pub(super) fn quick_agent_chat(&mut self, name: String, cx: &mut Context<Self>) {
        self.sync_conversation_spaces(cx);
        let started = name.clone();
        self.conversations.update(cx, |view, _| {
            view.set_agent_services(self.catalog.services.clone());
        });
        let result = if matches!(self.backend, Backend::Ready) {
            self.conversations.update(cx, |view, cx| view.begin_active_space_conversation(name, cx))
        } else {
            Err("Wait for the terminal service to connect.".into())
        };
        match result {
            Ok(_) => self.last_agent = Some(started),
            Err(error) => {
                self.problem = Some(error);
                cx.notify();
            }
        }
    }

    pub(super) fn activate_selected_conversation(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.backend, Backend::Ready) {
            return;
        }
        let target = self.conversations.read(cx).selected_row().and_then(|row| {
            let pane = chartr_herdr::PaneId(row.runtime.as_ref()?.clone());
            let space = self.spaces.iter().find(|space| {
                space.read(cx).session_item(&pane).is_some_and(|item| {
                    item.session.ended().is_none() && matches_session(row, &item.session.info)
                })
            })?;
            Some((space.clone(), pane))
        });
        let Some((space, pane)) = target else {
            return;
        };
        // Activate the existing item in its group/split; mode_focus_pending gives its
        // terminal keyboard focus once the destination workspace has rendered.
        if space.update(cx, |space, cx| space.activate_session(&pane, cx)) {
            self.collapsed_spaces.remove(&space.entity_id());
            self.active = Some(space);
        }
    }

    pub(super) fn sync_inbox_terminal(&mut self, cx: &mut Context<Self>) {
        let inbox = self.conversations.read(cx);
        let target = inbox.selected_runtime().and_then(|runtime| {
            let pane = chartr_herdr::PaneId(runtime.to_owned());
            let item = self.spaces.iter().find_map(|space| space.read(cx).session_item(&pane))?;
            if let Some(row) = inbox.selected_row()
                && !matches_session(row, &item.session.info)
            {
                return Some((
                    None,
                    Some("Session ended. This conversation remains in Chats.".to_owned()),
                ));
            }
            if item.session.ended().is_some() {
                return Some((
                    None,
                    Some("Session ended. This conversation remains in Chats.".to_owned()),
                ));
            }
            Some((item.terminal_view(), None))
        });
        let (view, notice) = target.unwrap_or_default();
        self.conversations.update(cx, |inbox, cx| inbox.set_terminal(view, notice, cx));
    }

    pub(super) fn sync_conversation_spaces(&mut self, cx: &mut Context<Self>) {
        let choices = self
            .spaces
            .iter()
            .map(|space| {
                let space = space.read(cx);
                crate::conversations::SpaceChoice {
                    key: space.key(),
                    name: space.name().into(),
                    path: (space.kind() == SpaceKind::Registered).then(|| space.path().clone()),
                }
            })
            .collect();
        let active = self.active.as_ref().map(|space| space.read(cx).key());
        self.conversations.update(cx, |view, cx| {
            if view.set_spaces(choices, active) {
                cx.notify();
            }
        });
    }

    pub(super) fn observe_conversations(
        &mut self,
        infos: &[chartr_herdr::control::Session],
        cx: &mut Context<Self>,
    ) {
        self.conversations.update(cx, |view, _| {
            view.set_agent_services(self.catalog.services.clone());
        });
        let observations = infos
            .iter()
            .filter_map(|session| {
                let provider = Provider::detect(session.agent.as_deref()?)?;
                let native = session
                    .agent_session
                    .as_ref()
                    .filter(|identity| Provider::detect(&identity.agent) == Some(provider))
                    .and_then(|identity| {
                        NativeSession::from_identity(provider, &identity.kind, &identity.value)
                    });
                let owner =
                    self.spaces
                        .iter()
                        .find(|space| space.read(cx).owns_session(&session.id))
                        .or_else(|| {
                            self.spaces.iter().find(|space| {
                                session.cwd.as_ref().is_some_and(|cwd| {
                                    spaces::same_path(space.read(cx).path(), cwd)
                                })
                            })
                        })
                        .map(|space| {
                            let space = space.read(cx);
                            chartr_conversations::SpaceIdentity {
                                key: space.key(),
                                name: space.name().into(),
                            }
                        });
                Some(Observation {
                    space: owner,
                    runtime: session.id.0.clone(),
                    terminal: session.terminal.0.clone(),
                    provider,
                    native,
                    cwd: session.cwd.clone(),
                    title: session.conversation_title.clone(),
                    status: match session.status {
                        chartr_herdr::control::SessionStatus::Working => Status::Working,
                        chartr_herdr::control::SessionStatus::Blocked => Status::Waiting,
                        chartr_herdr::control::SessionStatus::Idle
                        | chartr_herdr::control::SessionStatus::Done => Status::Idle,
                        chartr_herdr::control::SessionStatus::Unknown => Status::Unknown,
                    },
                    pid: session.foreground_pid,
                })
            })
            .collect();
        self.conversations.update(cx, |view, cx| view.observe(observations, cx));
    }

    pub(super) fn conversation_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Event::SelectionChanged => {
                if self.terminal_search_open {
                    self.close_terminal_search(window, cx);
                }
                self.schedule_persistence(cx);
            }
            Event::LaunchAgent { name, space } => {
                self.new_agent_conversation(name.clone(), space.clone(), window, cx)
            }
            Event::ManageAgents => {
                if let Some(handle) = window.window_handle().downcast::<Self>() {
                    crate::settings_window::open_plugin(
                        handle,
                        cx.weak_entity(),
                        Some("com.chartr.agent".into()),
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    fn new_agent_conversation(
        &mut self,
        name: String,
        space_key: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let preparation = (|| {
            let client = self
                .client
                .clone()
                .filter(|_| matches!(self.backend, Backend::Ready))
                .ok_or("Wait for the terminal service to connect.")?;
            let space = self
                .spaces
                .iter()
                .find(|space| space.read(cx).key() == space_key)
                .cloned()
                .ok_or("The selected space is no longer available. Choose a space again.")?;
            let launch = prepare_registered_conversation(&self.catalog.services, &name, cx)?;
            Ok::<_, String>((client, space, launch))
        })();
        let (client, space, launch) = match preparation {
            Ok(prepared) => prepared,
            Err(error) => {
                self.conversations.update(cx, |view, cx| view.report(Err(error), cx));
                return;
            }
        };
        let executor = cx.background_executor().clone();
        let window_handle = window.window_handle();
        let select_workspace_tab = agent_launch_selects_workspace_tab(self.mode);
        cx.spawn(async move |this, cx| {
            let install_client = client.clone();
            // Install/verify known providers before launch so SessionStart is not missed.
            let installed = executor
                .spawn(async move {
                    if let Some(provider) = launch.integration {
                        install_client
                            .install_agent_integration(&provider)
                            .map_err(|e| e.to_string())?;
                    }
                    Ok::<_, String>(())
                })
                .await;
            let result = async {
                installed?;
                this.update(cx, |this, _| {
                    if this.spaces.contains(&space) {
                        Ok(())
                    } else {
                        Err("The selected space was removed before launch.".to_owned())
                    }
                })
                .map_err(|e| e.to_string())??;
                this.update(cx, |this, cx| {
                    prepare_registered_conversation(&this.catalog.services, &name, cx)
                })
                .map_err(|e| e.to_string())??;
                let terminal =
                    space.update(cx, |space, cx| space.prepare_plugin_session(cx)).await?;
                if select_workspace_tab {
                    this.update(cx, |this, cx| {
                        space.update(cx, |space, cx| {
                            space.activate_session(&chartr_herdr::PaneId(terminal.id.clone()), cx)
                        });
                        if this.active.as_ref() == Some(&space) && this.mode != Mode::Inbox {
                            this.mode_focus_pending = true;
                            cx.notify();
                        }
                    })
                    .map_err(|e| e.to_string())?;
                }
                cx.update_window(window_handle, |_, window, cx| {
                    space.update(cx, |space, cx| {
                        space.size_conversation_terminal(
                            &chartr_herdr::PaneId(terminal.id.clone()),
                            window,
                            cx,
                        )
                    });
                })
                .map_err(|e| e.to_string())?;
                let launch = this
                    .update(cx, |this, cx| {
                        this.conversations
                            .update(cx, |view, cx| view.launch_allocated(&name, &terminal.id, cx));
                        // Resolve again after async startup: disabled/deleted profiles must not run.
                        prepare_registered_conversation(&this.catalog.services, &name, cx)
                    })
                    .map_err(|e| e.to_string())??;
                terminal.send(&launch.input)?;
                this.update(cx, |this, cx| {
                    this.conversations
                        .update(cx, |view, cx| view.launch_started(&name, &terminal.id, cx));
                })
                .map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.conversations.update(cx, |view, cx| view.report(result, cx));
                this.refresh(cx);
            });
        })
        .detach();
    }
}

fn agent_launch_selects_workspace_tab(mode: Mode) -> bool {
    mode != Mode::Inbox
}

fn prepare_registered_conversation(
    services: &chartr_plugin::services::Services,
    name: &str,
    cx: &App,
) -> Result<chartr_plugin::services::InboxLaunch, String> {
    use chartr_plugin::services::{AGENT_SERVICE, Agents, InboxAgents, InboxLaunch};
    if let Some(service) = services.get::<InboxAgents>(AGENT_SERVICE) {
        return service.prepare(name, cx);
    }
    let service =
        services.get::<Agents>(AGENT_SERVICE).ok_or("Enable Agent and register an agent first.")?;
    Ok(InboxLaunch { input: service.prepare(name, "", cx)?, integration: None })
}

fn resume_preflight(
    native: Option<&NativeSession>,
    transcript_exists: bool,
    live: bool,
) -> Result<(), String> {
    let native = native.ok_or("No native session ID was recorded.")?;
    if native.id.is_empty() {
        return Err("No native session ID was recorded.".into());
    }
    if !transcript_exists {
        return Err("The transcript is missing.".into());
    }
    if live {
        return Err("This conversation is already live in another terminal.".into());
    }
    Ok(())
}

/// Reused panes must never expose another native conversation through an old row.
fn matches_session(
    row: &chartr_conversations::Conversation,
    session: &chartr_herdr::control::Session,
) -> bool {
    row.runtime.as_deref() == Some(&session.id.0)
        && row.terminal.as_deref() == Some(&session.terminal.0)
        && session.agent.as_deref().and_then(Provider::detect) == Some(row.provider)
        && row.native.as_ref().is_none_or(|native| {
            session.agent_session.as_ref().is_some_and(|identity| {
                Provider::detect(&identity.agent) == Some(row.provider)
                    && NativeSession::from_identity(row.provider, &identity.kind, &identity.value)
                        .is_some_and(|observed| {
                            observed.id == native.id
                                && (native.path.is_none() || observed.path == native.path)
                        })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_launch_selects_workspace_tab_only_outside_inbox() {
        assert!(agent_launch_selects_workspace_tab(Mode::Sidebar));
        assert!(agent_launch_selects_workspace_tab(Mode::Tabs));
        assert!(!agent_launch_selects_workspace_tab(Mode::Inbox));
    }

    fn session() -> chartr_herdr::control::Session {
        chartr_herdr::control::Session {
            id: chartr_herdr::PaneId("pane".into()),
            terminal: chartr_herdr::TerminalId("pty".into()),
            workspace: chartr_herdr::WorkspaceId("workspace".into()),
            label: "agent".into(),
            running: None,
            status: chartr_herdr::control::SessionStatus::Idle,
            agent: Some("codex".into()),
            agent_session: Some(chartr_herdr::protocol::AgentSession {
                source: "integration".into(),
                agent: "codex".into(),
                kind: "id".into(),
                value: "native-a".into(),
            }),
            conversation_title: None,
            foreground_pid: None,
            cwd: None,
        }
    }

    #[test]
    fn a_resume_identity_has_only_one_writer_across_windows() {
        let native = NativeSession { id: "shared-resume-lease-test".into(), path: None };
        let first = ResumeLease::reserve(Provider::Pi, native.clone()).unwrap();
        assert!(ResumeLease::reserve(Provider::Pi, native.clone()).is_err());
        drop(first);
        assert!(ResumeLease::reserve(Provider::Pi, native).is_ok());
    }

    #[test]
    fn resume_refuses_unknown_id_missing_transcript_and_live_writer() {
        let native = NativeSession { id: "exact-id".into(), path: None };
        assert!(resume_preflight(None, true, false).unwrap_err().contains("native session ID"));
        assert!(resume_preflight(Some(&native), false, false).unwrap_err().contains("transcript"));
        assert!(resume_preflight(Some(&native), true, true).unwrap_err().contains("already live"));
        assert!(resume_preflight(Some(&native), true, false).is_ok());
    }

    #[test]
    fn inbox_binds_added_provider_identities_and_rejects_a_changed_session() {
        for (agent, provider, kind, value) in [
            (
                "omp",
                Provider::Omp,
                "path",
                "/omp/profile/sessions/project/2026-09-14_native-a.jsonl",
            ),
            ("cursor", Provider::Cursor, "id", "native-a"),
            ("agy", Provider::Antigravity, "id", "native-a"),
        ] {
            let mut session = session();
            session.agent = Some(agent.into());
            session.agent_session = Some(chartr_herdr::protocol::AgentSession {
                source: format!("herdr:{}", provider.slug()),
                agent: agent.into(),
                kind: kind.into(),
                value: value.into(),
            });
            let native = NativeSession::from_identity(provider, kind, value).unwrap();
            assert_eq!(native.id, "native-a");
            let mut row: chartr_conversations::Conversation =
                serde_json::from_value(serde_json::json!({
                    "id":"row", "provider":provider, "native":native,
                    "title":"Task", "cwd":null, "updated":1,
                    "draft":"", "archived":false, "messages":[]
                }))
                .unwrap();
            row.runtime = Some("pane".into());
            row.terminal = Some("pty".into());
            assert!(matches_session(&row, &session));
            session.agent_session.as_mut().unwrap().value = value.replace("native-a", "native-b");
            assert!(!matches_session(&row, &session));
        }
    }

    #[test]
    fn inbox_binds_pi_path_identities_without_following_another_session() {
        let mut session = session();
        session.agent = Some("pi".into());
        let identity = session.agent_session.as_mut().unwrap();
        identity.agent = "pi".into();
        identity.kind = "path".into();
        identity.value = "/pi/sessions/--project--/2026-09-11T14-00-00-000Z_pi-a.jsonl".into();
        let native =
            NativeSession::from_identity(Provider::Pi, &identity.kind, &identity.value).unwrap();
        assert_eq!(native.id, "pi-a");
        let mut row: chartr_conversations::Conversation =
            serde_json::from_value(serde_json::json!({
                "id":"a", "provider":"pi", "native":native,
                "title":"Axolotls", "cwd":null, "updated":1,
                "draft":"", "archived":false, "messages":[]
            }))
            .unwrap();
        row.runtime = Some("pane".into());
        row.terminal = Some("pty".into());
        assert!(matches_session(&row, &session));
        session.agent_session.as_mut().unwrap().value =
            "/pi/sessions/--project--/2026-09-11T14-00-00-000Z_pi-b.jsonl".into();
        assert!(!matches_session(&row, &session));
        assert!(
            NativeSession::from_identity(Provider::Pi, "path", "relative/session_a.jsonl")
                .is_none()
        );
        assert!(NativeSession::from_identity(Provider::Pi, "path", "/unknown/file.txt").is_none());
        assert!(
            NativeSession::from_identity(Provider::Claude, "path", "/pi/session_a.jsonl").is_none()
        );
    }

    #[test]
    fn inbox_terminal_binding_rejects_reused_panes_and_changed_native_sessions() {
        let mut row: chartr_conversations::Conversation =
            serde_json::from_value(serde_json::json!({
                "id":"a", "provider":"codex", "native":{"id":"native-a", "path":null},
                "title":"Task", "custom_title":null, "cwd":null, "updated":1,
                "draft":"", "archived":false, "messages":[]
            }))
            .unwrap();
        row.runtime = Some("pane".into());
        row.terminal = Some("pty".into());
        assert!(matches_session(&row, &session()));
        let mut changed = session();
        changed.agent_session.as_mut().unwrap().value = "native-b".into();
        assert!(!matches_session(&row, &changed));
        changed = session();
        changed.terminal.0 = "replacement-pty".into();
        assert!(!matches_session(&row, &changed));
        changed = session();
        changed.agent = None;
        assert!(!matches_session(&row, &changed));
        changed = session();
        changed.agent_session = None;
        assert!(!matches_session(&row, &changed));
        row.native = None;
        assert!(
            matches_session(&row, &changed),
            "Detected agents can use their terminal before integration"
        );
        row.runtime = None;
        assert!(
            !matches_session(&row, &session()),
            "Ended history cannot fall back to an active pane"
        );
    }
}
