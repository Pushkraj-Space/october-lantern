use super::*;

impl Lantern {
    pub(super) fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).max_size(egui::vec2(32.0, 32.0)));
            ui.vertical(|ui| {
                ui.label(RichText::new("lantern").size(22.0).strong());
                ui.label(RichText::new("OCTOBER  /  LINUX").size(9.0).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+  New session").clicked() {
                    self.show_new = true;
                    self.cancel_speech();
                }
                if ui.button("Compact").on_hover_text("A small floating window for your agents").clicked() {
                    self.set_compact(true, ui.ctx());
                }
                if ui.small_button("Refresh").clicked() {
                    self.request(json!({"type":"refresh"}));
                }
            });
        });
    }
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("YOUR WORKSPACE").size(10.0).color(MUTED));
            ui.label(RichText::new(format!("{}", self.agents.len())).color(ACCENT));
        });
        for (page, label) in [
            (Page::Sessions, "Sessions"),
            (Page::Ask, "Point & Ask"),
            (Page::Connections, "Connections"),
            (Page::Settings, "Settings"),
            (Page::Help, "Help & about"),
        ] {
            if ui.selectable_label(self.page == page, RichText::new(label).size(14.0)).clicked() {
                if self.page != page {
                    self.cancel_speech();
                }
                self.page = page;
            }
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Search agents or projects").desired_width(f32::INFINITY));
        ui.checkbox(&mut self.needs_only, "Needs attention");
        let filter = self.filter.to_lowercase();
        let agents = self.agents.clone();
        egui::ScrollArea::vertical().id_salt("agent-list").show(ui, |ui| {
            for agent in agents {
                if self.needs_only && agent["state"] != "needs_input" && agent["state"] != "waiting" {
                    continue;
                }
                if !format!("{} {} {}", string(&agent, "handle"), string(&agent, "project"), string(&agent, "cwd"))
                    .to_lowercase()
                    .contains(&filter)
                {
                    continue;
                }
                let selected = self.selected.as_deref() == Some(string(&agent, "id"));
                let fill = if selected { ui.visuals().selection.bg_fill } else { ui.visuals().faint_bg_color };
                let response = egui::Frame::new()
                    .fill(fill)
                    .inner_margin(12.0)
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width((ui.available_width() - 4.0).max(60.0));
                        ui.horizontal(|ui| {
                            dot(ui, state_color(string(&agent, "state")));
                            ui.label(RichText::new(short(string(&agent, "handle"), 24)).strong());
                        });
                        ui.label(short(string(&agent, "project"), 30));
                        ui.label(
                            RichText::new(format!("{} · {}", string(&agent, "kind"), state_label(string(&agent, "state"))))
                                .small()
                                .color(MUTED),
                        );
                    })
                    .response;
                if ui.interact(response.rect, response.id.with("select"), egui::Sense::click()).clicked() {
                    self.select(string(&agent, "id").into());
                }
            }
        });
    }
    pub(super) fn conversation(&mut self, ui: &mut egui::Ui) {
        let Some(agent) = self.agents.iter().find(|agent| Some(string(agent, "id")) == self.selected.as_deref()).cloned() else {
            ui.add_space(70.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new(&self.logo).max_size(egui::vec2(92.0, 92.0)));
                ui.heading("A little closer to your agents");
                ui.label("See their progress, answer questions, and keep work moving.");
                ui.add_space(14.0);
                if ui.button("Start your first session").clicked() {
                    self.show_new = true;
                }
                ui.label(
                    RichText::new(if cfg!(windows) {
                        "Sessions you start here take replies. Agents started in your own terminal are followed, not typed into."
                    } else {
                        "Agents in tmux support direct replies on Linux."
                    })
                    .small()
                    .color(MUTED),
                );
            });
            return;
        };
        let id = string(&agent, "id").to_owned();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(string(&agent, "handle"));
                ui.label(RichText::new(string(&agent, "cwd")).small().color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // On Windows, Open brings the agent's terminal window to the front.
                let focusable = agent["tmux"].is_object()
                    || agent["route"]["via"] == "october"
                    || agent["route"]["via"] == "pty"
                    || cfg!(windows) && agent["host"].is_object();
                if ui.add_enabled(focusable, egui::Button::new("Open terminal")).clicked() {
                    os::allow_focus(self.engine.pid());
                    self.request(json!({"type":"focus","agentId":id}));
                }
                ui.label(RichText::new(state_label(string(&agent, "state"))).color(state_color(string(&agent, "state"))));
            });
        });
        ui.add_space(4.0);
        ui.separator();
        if !string(&agent, "question").is_empty() {
            egui::Frame::new().fill(ui.visuals().selection.bg_fill).inner_margin(14.0).corner_radius(8.0).show(ui, |ui| {
                ui.label(RichText::new("YOUR AGENT NEEDS YOU").size(10.0).color(ACCENT));
                ui.label(string(&agent, "question"));
                if !string(&agent, "questionDetail").is_empty() {
                    egui::CollapsingHeader::new("Details").show(ui, |ui| {
                        ui.monospace(string(&agent, "questionDetail"));
                    });
                }
                if agent["questionKind"] == "permission" && !string(&agent, "promptId").is_empty() {
                    ui.horizontal(|ui| {
                        for (label, key) in [("Allow once", "1"), ("Deny", "Escape")] {
                            if ui
                                .add_enabled(
                                    self.connected && agent["canReply"] == true && self.pending_reply.is_none(),
                                    egui::Button::new(label),
                                )
                                .clicked()
                                && let Some(request) =
                                    self.request(json!({"type":"keys","agentId":id,"keys":[key],"promptId":string(&agent,"promptId")}))
                            {
                                self.pending_reply = Some(PendingReply { request, agent: id.clone(), draft: None });
                                self.status = "Answering permission request…".into();
                            }
                        }
                    });
                }
            });
        }
        let attachment_height = if self.drafts.get(&id).is_some_and(|draft| draft.screenshot.is_some()) { 105.0 } else { 0.0 };
        let history_width = ui.available_width();
        let history_height = (ui.available_height() - 155.0 - attachment_height).max(70.0);
        egui::ScrollArea::vertical()
            .id_salt(("history", &id))
            .max_width(history_width)
            .auto_shrink([false, false])
            .max_height(history_height)
            .min_scrolled_height(history_height)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_width(history_width);
                if self.messages.is_empty() {
                    ui.label(if string(&agent, "lastMessage").is_empty() {
                        "The conversation will appear here as your agent works."
                    } else {
                        string(&agent, "lastMessage")
                    });
                }
                for (index, message) in self.messages.iter().enumerate() {
                    egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(14.0).corner_radius(8.0).show(ui, |ui| {
                        ui.set_width((history_width - 28.0).max(10.0));
                        ui.label(RichText::new(if string(message, "role") == "user" { "YOU" } else { "AGENT" }).size(10.0).color(ACCENT));
                        ui.push_id(index, |ui| {
                            egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.markdown, string(message, "text"));
                        });
                    });
                }
            });
        ui.separator();
        self.screenshot_preview(ui, &id);
        let draft = self.drafts.entry(id.clone()).or_default();
        ui.add(
            egui::TextEdit::multiline(&mut draft.text)
                .hint_text("Send a thought, an answer, or the next step…")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            let ready = self.connected
                && agent["canReply"] == true
                && self.pending_reply.is_none()
                && self.drafts.get(&id).is_some_and(|draft| !draft.text.trim().is_empty());
            if ui.add_enabled(ready, egui::Button::new("Send reply  ↗").fill(ui.visuals().selection.bg_fill)).clicked()
                || ready && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter))
            {
                self.send_reply(&agent);
            }
            if ui.add_enabled(!self.capture_pending, egui::Button::new("Screenshot")).clicked() {
                self.capture(id.clone());
            }
            if ui
                .add_enabled(!self.speech_busy, egui::Button::new(if self.recording.is_some() { "Stop dictation" } else { "Dictate" }))
                .clicked()
            {
                self.dictation(id.clone());
            }
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(self.drafts.get(&id).map(|draft| draft.text.clone()).unwrap_or_default());
            }
            if self.pending_reply.is_some() || self.speech_busy {
                ui.spinner();
            }
        });
        if agent["canReply"] != true {
            ui.label(
                RichText::new(if cfg!(windows) {
                    "Lantern only types into sessions it started. Reply in the terminal, or copy your draft above."
                } else {
                    "Reply in the terminal for this session. You can copy your draft above."
                })
                .small()
                .color(MUTED),
            );
        } else {
            ui.label(RichText::new("Ctrl + Enter to send · drafts stay with each session").small().color(MUTED));
        }
    }
    pub(super) fn connections(&mut self, ui: &mut egui::Ui) {
        ui.heading("Connections");
        ui.label(RichText::new("Your desktop, your account, and your phone.").color(MUTED));
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.group(|ui| {
                ui.set_min_width((ui.available_width() - 2.0).max(10.0));
                ui.heading("October account");
                if let Some(session) = &self.session {
                    ui.label(&session.email);
                    if !session.plan.is_null() {
                        ui.label(
                            RichText::new(format!(
                                "Plan: {}",
                                session.plan["plan"].as_str().or(session.plan["tier"].as_str()).unwrap_or("connected")
                            ))
                            .color(MUTED),
                        );
                    }
                    if ui.button("Sign out").clicked() {
                        self.sign_out();
                    }
                } else {
                    ui.label("Sign in to use October AI and connect your phone.");
                    ui.add_enabled_ui(!self.auth_pending, |ui| {
                        ui.horizontal(|ui| {
                            for (label, provider) in [("Google", "google"), ("GitHub", "github"), ("Apple", "apple")] {
                                if ui.button(label).clicked() {
                                    self.auth(move |generation, current| account::oauth(provider, generation, current));
                                }
                            }
                        });
                        ui.add(egui::TextEdit::singleline(&mut self.email).hint_text("Email").desired_width(300.0));
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true).hint_text("Password").desired_width(300.0));
                        ui.checkbox(&mut self.remember, "Remember sign-in in the system keyring");
                        if ui
                            .add_enabled(
                                !self.email.trim().is_empty() && !self.password.is_empty(),
                                egui::Button::new("Sign in with email"),
                            )
                            .clicked()
                        {
                            let email = self.email.trim().to_owned();
                            let password = std::mem::take(&mut self.password);
                            self.auth(move |_, _| account::password(email, password));
                        }
                    });
                    if self.auth_pending {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Waiting for sign-in…");
                            if ui.button("Cancel").clicked() {
                                self.auth_generation.fetch_add(1, Ordering::SeqCst);
                                self.auth_pending = false;
                                self.status = "Sign-in cancelled".into();
                            }
                        });
                    }
                }
            });
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.set_min_width((ui.available_width() - 2.0).max(10.0));
                ui.heading("Your phone");
                ui.label("Check on your agents and reply from the October phone app.");
                if self.session.is_none() {
                    ui.label(RichText::new("Sign in to October on both devices to pair.").color(MUTED));
                } else {
                    ui.label(format!(
                        "Status: {}",
                        if string(&self.phone, "status").is_empty() { "Connecting…" } else { string(&self.phone, "status") }
                    ));
                    if !string(&self.phone, "message").is_empty() {
                        ui.label(string(&self.phone, "message"));
                    }
                    let pairing = self.phone["pairing"].clone();
                    if pairing.is_object() {
                        if let Some(code) = pairing["code"].as_str() {
                            ui.label(format!("Does {} show the same code?", pairing["label"].as_str().unwrap_or("your phone")));
                            ui.label(RichText::new(code).size(32.0).strong().color(ACCENT));
                            if pairing["finishing"] == true {
                                ui.spinner();
                                ui.label("Finishing secure pairing…");
                            } else {
                                ui.horizontal(|ui| {
                                    for (label, allow) in [("Codes match", true), ("Reject", false)] {
                                        if ui.button(label).clicked() {
                                            self.request(json!({"type":"phone.decide","allow":allow}));
                                        }
                                    }
                                });
                            }
                        } else if let Some(qr) = pairing["qr"].as_str() {
                            draw_qr(ui, qr);
                            ui.label("In October on your phone, choose Pair a computer and scan this code.");
                        }
                        if ui.button("Cancel pairing").clicked() {
                            self.request(json!({"type":"phone.cancelPair"}));
                        }
                    } else if ui.add_enabled(self.connected, egui::Button::new("Pair a phone")).clicked() {
                        self.request(json!({"type":"phone.pair"}));
                    }
                    for device in self.phone["devices"].as_array().cloned().unwrap_or_default() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} · {}", string(&device, "label"), string(&device, "platform")));
                            if ui.button("Revoke").clicked() {
                                self.request(json!({"type":"phone.revoke","bind":string(&device,"bind")}));
                            }
                        });
                    }
                }
            });
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.set_min_width((ui.available_width() - 2.0).max(10.0));
                ui.heading("October Desktop");
                ui.label(format!("Status: {}", string(&self.october, "status")));
                if !string(&self.october, "message").is_empty() {
                    ui.label(string(&self.october, "message"));
                }
                if let Some(code) = self.october["pairingCode"].as_str() {
                    ui.label(RichText::new(code).size(28.0).color(ACCENT));
                    ui.label("Compare this code in October Desktop.");
                    if ui.button("Cancel").clicked() {
                        self.request(json!({"type":"october.cancelPair"}));
                    }
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.connected && self.october["status"] != "notInstalled" && self.october["status"] != "notRunning",
                                egui::Button::new("Pair October"),
                            )
                            .clicked()
                        {
                            self.request(json!({"type":"october.pair"}));
                        }
                        if self.october["paired"] == true && ui.button("Forget connection").clicked() {
                            self.request(json!({"type":"october.forget"}));
                        }
                    });
                }
            });
        });
    }
    pub(super) fn point_ask(&mut self, ui: &mut egui::Ui) {
        ui.heading("Point & Ask");
        ui.label(RichText::new("A second pair of eyes for anything on your screen.").color(MUTED));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.capture_pending, egui::Button::new("Capture screen, window or area")).clicked() {
                self.capture("__ask".into());
            }
            if ui
                .add_enabled(
                    !self.speech_busy,
                    egui::Button::new(if self.recording.is_some() { "Stop dictation" } else { "Dictate question" }),
                )
                .clicked()
            {
                self.dictation("__ask".into());
            }
        });
        self.screenshot_preview(ui, "__ask");
        ui.add(
            egui::TextEdit::multiline(&mut self.ask_draft.text)
                .hint_text("What would you like to know?")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        egui::CollapsingHeader::new("Model").show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.ask_model).hint_text("Automatic · or enter an October model ID"));
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.session.is_some() && !self.ask_pending && !self.ask_draft.text.trim().is_empty(),
                    egui::Button::new("Ask October  ↗"),
                )
                .clicked()
            {
                self.cancel_speech();
                let session = self.session.as_ref().expect("enabled sign-in");
                let access = session.access.clone();
                let draft = self.ask_draft.clone();
                let model = self.ask_model.clone();
                self.ask_generation.fetch_add(1, Ordering::SeqCst);
                let generation = self.ask_generation.load(Ordering::SeqCst);
                self.ask_pending = true;
                self.ask_answer.clear();
                self.status = "October is thinking…".into();
                let sink_tx = self.tx.clone();
                let current = self.ask_generation.clone();
                let history = self.ask_history.clone();
                self.ask_question = draft.text.clone();
                os::task(
                    self.tx.clone(),
                    move || {
                        account::ask(access, draft.text, draft.screenshot, model, history, move |text| {
                            current.load(Ordering::SeqCst) == generation
                                && sink_tx.send(Event::AskDelta { generation, text: text.to_string() }).is_ok()
                        })
                    },
                    move |result| Event::Ask { generation, result },
                );
            }
            if self.ask_pending {
                ui.spinner();
                if ui.button("Cancel answer").clicked() {
                    self.ask_generation.fetch_add(1, Ordering::SeqCst);
                    self.ask_pending = false;
                }
            }
            if self.session.is_none() && ui.button("Sign in to October").clicked() {
                self.page = Page::Connections;
            }
        });
        ui.label(
            RichText::new("Your question and attached image are sent to October when you ask. Uses your October plan.")
                .small()
                .color(MUTED),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if self.ask_agent.is_empty() {
                self.ask_agent = self.selected.clone().unwrap_or_default();
            }
            let title = self
                .agents
                .iter()
                .find(|agent| string(agent, "id") == self.ask_agent)
                .map(|agent| string(agent, "handle"))
                .unwrap_or("Choose a session");
            egui::ComboBox::from_id_salt("forward-agent").selected_text(title).show_ui(ui, |ui| {
                for agent in &self.agents {
                    ui.selectable_value(&mut self.ask_agent, string(agent, "id").into(), string(agent, "handle"));
                }
            });
            if ui
                .add_enabled(
                    !self.ask_agent.is_empty() && !self.ask_draft.text.trim().is_empty() && !self.ask_pending,
                    egui::Button::new("Continue in session"),
                )
                .clicked()
            {
                let id = self.ask_agent.clone();
                let question = self.ask_draft.text.clone();
                let answer = self.ask_answer.clone();
                let screenshot = self.ask_draft.screenshot.clone();
                let draft = self.draft_for(&id);
                if !draft.text.is_empty() {
                    draft.text.push_str("\n\n");
                }
                draft.text.push_str(&question);
                if !answer.is_empty() {
                    draft.text.push_str("\n\nOctober's answer:\n");
                    draft.text.push_str(&answer);
                }
                if screenshot.is_some() {
                    draft.screenshot = screenshot;
                }
                self.select(id);
            }
            if ui.button("Clear conversation").clicked() {
                self.ask_generation.fetch_add(1, Ordering::SeqCst);
                self.ask_pending = false;
                self.ask_history.clear();
                self.ask_answer.clear();
                self.ask_draft = Default::default();
                self.capture_generation += 1;
            }
        });
        ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt("ask-answer").show(ui, |ui| {
            if !self.ask_answer.is_empty() {
                ui.label(RichText::new("OCTOBER").size(10.0).color(ACCENT));
                egui_commonmark::CommonMarkViewer::new().show(ui, &mut self.markdown, &self.ask_answer);
                if ui.button("Copy answer").clicked() {
                    ui.ctx().copy_text(self.ask_answer.clone());
                }
            }
        });
    }
    pub(super) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Make Lantern yours");
        ui.label(RichText::new("Desktop behavior, agent tools, and local speech.").color(MUTED));
        ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt("settings").show(ui,|ui|{
            ui.group(|ui|{
                ui.set_min_width((ui.available_width()-2.0).max(10.0));ui.strong("Appearance & window");
                ui.checkbox(&mut self.settings.light,"Light theme");ui.add(egui::Slider::new(&mut self.settings.scale,0.8..=1.6).text("UI scale"));
                ui.add_enabled(self.can_hide, egui::Checkbox::new(&mut self.settings.always_on_top,"Keep window on top"));
                if cfg!(target_os="linux"){
                ui.label(RichText::new("Wayland lets your compositor decide window position and stacking.").small().color(MUTED));
if ui.button("Install GNOME edge lantern").clicked(){os::task(self.tx.clone(),os::edge_install,|result|Event::Status(result.unwrap_or_else(|error|error)));}
ui.label(RichText::new("Optional GNOME extension: a hovering edge control with your agent list across workspaces.").small().color(MUTED));}
                ui.add_enabled(self.tray,egui::Checkbox::new(&mut self.settings.close_to_tray,if self.can_hide {"Close to system tray"} else {"Minimize on close (Wayland)"}));
                if !self.can_hide {ui.label(RichText::new("Wayland controls window focus and stacking. The GNOME edge extension can restore Lantern; the overview also works.").small().color(MUTED));}
                if !self.tray{ui.label(RichText::new("No tray host found. Use Compact, or enable an AppIndicator extension in GNOME.").small().color(MUTED));}
                let mut autostart=settings::autostart_enabled();
if ui.checkbox(&mut autostart,"Start when I log in").changed() && let Err(error)=settings::autostart(autostart){self.status=format!("Autostart: {error:#}");}
                if ui.button(if cfg!(windows){"Add Lantern to the Start menu"}else{"Add Lantern to the application launcher"}).clicked(){self.status=match std::env::current_exe().map(|path|path.parent().unwrap().to_path_buf()){
                    Ok(bundle)=>match updates::install_launcher(&bundle){Ok(())=>"Shortcut added".into(),Err(error)=>format!("{error:#}")},Err(error)=>error.to_string()};}
            });ui.add_space(10.0);
            ui.group(|ui|{
                ui.set_min_width((ui.available_width()-2.0).max(10.0));ui.strong("Notifications & shortcuts");ui.checkbox(&mut self.settings.notifications,"Notify when an agent needs input");ui.checkbox(&mut self.settings.notify_finished,"Also notify when a turn finishes");
                if ui.add_enabled(!self.shortcuts&&!self.shortcuts_pending,egui::Button::new(if self.shortcuts{"Shortcuts enabled"}else{"Enable global shortcuts…"})).clicked(){self.shortcuts_pending=true;os::shortcuts(self.tx.clone());}
                if self.shortcuts_pending{ui.spinner();}
                ui.label(RichText::new(if cfg!(windows) {"Ctrl+Alt+L shows Lantern from anywhere."} else {"Your desktop asks you to approve the keys. Suggested: Ctrl+Alt+L to show, Ctrl+Alt+P to capture."}).small().color(MUTED));
                if ui.button("Send a test notification").clicked(){os::notify(self.tx.clone(),"show".into(),"Lantern is ready".into(),"Click to open your workspace.".into());}
            });ui.add_space(10.0);
            ui.group(|ui|{
                ui.set_min_width((ui.available_width()-2.0).max(10.0));ui.strong("Agent tools");ui.checkbox(&mut self.settings.toolkit,"Share this computer's toolkit with new agents");ui.checkbox(&mut self.settings.bus,"Connect new agents through October Bus");
                ui.horizontal(|ui|{if ui.button("Refresh toolkit").clicked(){self.request(json!({"type":"toolkit.refresh"}));}
if ui.button("Prepare Bus").clicked(){self.request(json!({"type":"bus.prepare"}));}});
                if cfg!(target_os="linux"){ui.label("Terminal executable (optional; takes effect after reconnect)");ui.add(egui::TextEdit::singleline(&mut self.settings.terminal).hint_text("Automatic · ptyxis, kgx, konsole, kitty…").desired_width(f32::INFINITY));}
                ui.horizontal(|ui|{
                    for (label,action) in [("Hook status","status"),("Install agent hooks","install"),("Remove agent hooks","uninstall")]{if ui.button(label).clicked(){os::task(self.tx.clone(),move||os::hooks(action),|result|Event::Status(result.unwrap_or_else(|error|error)));}}
                });ui.label(RichText::new("Hooks improve permission detection. Installing them updates your agent configuration with backups.").small().color(MUTED));
            });ui.add_space(10.0);
            ui.group(|ui|{
                ui.set_min_width((ui.available_width()-2.0).max(10.0));ui.strong("Local dictation");
                ui.label("whisper.cpp executable");ui.text_edit_singleline(&mut self.settings.whisper);ui.label("Model file (.bin)");ui.add(egui::TextEdit::singleline(&mut self.settings.whisper_model).desired_width(f32::INFINITY));ui.label("Language (auto, en, hi…)");ui.text_edit_singleline(&mut self.settings.speech_language);
                ui.label(RichText::new(if cfg!(windows){"Audio is recorded from your default microphone and transcribed on this computer. Run scripts/setup-dictation.ps1 to install the local speech runtime."}else{"Audio is recorded through PipeWire and transcribed on this computer. Run scripts/setup-dictation.sh to install the local speech runtime."}).small().color(MUTED));
            });ui.add_space(10.0);
            ui.group(|ui|{
                ui.set_min_width((ui.available_width()-2.0).max(10.0));ui.strong(format!("October Lantern {}",env!("CARGO_PKG_VERSION")));
ui.checkbox(&mut self.settings.check_updates,"Check for updates daily");
                if ui.add_enabled(!self.update_pending,egui::Button::new(format!("Check for {} updates",updates::system_name()))).clicked(){self.update_pending=true;os::task(self.tx.clone(),updates::check,Event::Update);}
                if self.update_pending{ui.spinner();}
                if let Some(release)=self.update.clone(){ui.label(format!("Published release: {}",release.version));
if release.is_newer()
    && ui.add_enabled(!self.update_pending && self.pending_reply.is_none() && self.launch_pending.is_none(),egui::Button::new("Install update & restart")).clicked(){self.install_update(release);}}

                ui.label(RichText::new(if cfg!(windows) {"Updates are downloaded and installed only when you choose. Published releases must include a Windows zip and SHA-256 file."} else {"Updates are downloaded and installed only when you choose. Published releases must include a Linux bundle and SHA-256 file."}).small().color(MUTED));
            });ui.add_space(10.0);
            ui.horizontal(|ui|{
                if ui.button("Save preferences").clicked(){self.settings_error=self.settings.save().err().map(|error|format!("Could not save preferences: {error:#}"));self.status=self.settings_error.clone().unwrap_or_else(||"Preferences saved".into());self.appearance(ui.ctx());}
                if ui.add_enabled(self.pending_reply.is_none()&&self.launch_pending.is_none(),egui::Button::new("Reconnect engine")).clicked(){
                    match Sidecar::with_terminal(&self.settings.terminal){Ok(engine)=>{self.reconnect(engine);},Err(error)=>self.status=format!("{error:#}")}
                }
            });
if let Some(error)=&self.settings_error{ui.colored_label(ACCENT,error);}
        });
    }
    pub(super) fn welcome(&mut self, ctx: &egui::Context) {
        if !self.show_welcome {
            return;
        }
        egui::Modal::new(egui::Id::new("welcome")).show(ctx,|ui|{
            ui.set_width(470.0);ui.add(egui::Image::new(&self.logo).max_size(egui::vec2(70.0,70.0)));
            ui.heading(format!("Welcome to Lantern on {}",updates::system_name()));ui.label("Stay close to the work, wherever your agents run.");
            ui.add_space(12.0);
            for (title,body) in [("1. Bring your agents","Start a session here, or run an agent in a terminal. Lantern discovers it automatically."),
                if cfg!(windows){("2. Keep the conversation moving","Sessions started here take your replies and Allow/Deny answers. Agents you started in your own terminal are followed; Open brings their window to the front.")}
                else{("2. Keep the conversation moving","For direct replies, run agents in tmux. Drafts, screenshots and dictation stay with the session you chose.")},
                if cfg!(windows){("3. Settle into your desktop","Settings has notifications, a global shortcut, compact mode and the tray. Connections pairs October and your phone.")}
                else{("3. Settle into your desktop","Settings has notifications, global shortcuts, compact mode and an optional GNOME edge lantern. Connections pairs October and your phone.")}]{
                ui.strong(title);ui.label(body);ui.add_space(8.0);
            }
            ui.label(RichText::new("Local monitoring needs no October account. Screenshots are captured only when you choose; speech stays on this computer.").small().color(MUTED));
            if ui.button("Open my workspace").clicked(){self.show_welcome=false;self.settings.welcome_seen=true;if let Err(error)=self.settings.save(){self.status=format!("Could not save welcome preference: {error:#}");}}
        });
    }
    pub(super) fn help(&mut self, ui: &mut egui::Ui) {
        ui.heading("A little help");
        ui.label(RichText::new(format!("October Lantern {} · native Rust desktop + Rust sidecar", env!("CARGO_PKG_VERSION"))).color(MUTED));
        egui::ScrollArea::vertical().show(ui,|ui|{
            if cfg!(windows){ui.strong("Direct replies");ui.label("Start a session from Lantern (Ctrl+N) to reply to it from here. Lantern runs the agent itself and shows it in Windows Terminal (or a console window) when you choose Open terminal; closing that window leaves the agent running. Agents started in your own terminal are followed, but Lantern won't type into them: copy a draft instead.");}
            else{ui.strong("Direct replies");ui.label("Use tmux for agent sessions you start in a terminal. Lantern opens an attached terminal when you choose Open terminal. Other terminals support copying a draft.");
            ui.monospace("tmux new-session -s my-project\n# Then run claude, codex, or another supported agent.");}
            ui.strong("Desktop controls");ui.label("Ctrl+N opens a session. Ctrl+Enter sends a reply. Ctrl+, opens settings. Enable global shortcuts in Settings and approve your preferred keys in the desktop dialog.");
            ui.strong("Local speech");ui.label("The multilingual whisper.cpp model runs on your CPU. Set its executable, model and language in Settings. Stop dictation adds text to the draft; switching sessions cancels it.");
            ui.strong("Your phone & October AI");ui.label("Sign in under Connections. Phone pairing needs an October plan that includes mobile and the same account on both devices. Compare the six-digit code before approving. Live account and phone flows require those devices and credentials.");
            if cfg!(target_os="linux"){ui.strong("GNOME & Wayland");ui.label("The screenshot chooser belongs to your desktop. The optional edge extension works across workspaces; enabling a newly installed extension may require logging out and back in. Compact mode works without a tray extension.");}
            ui.horizontal(|ui|{
                if ui.button("Show welcome guide").clicked(){self.show_welcome=true;}
                if ui.button("Report a problem").clicked(){
                    let mut url=url::Url::parse("https://github.com/Pushkraj-Space/october-lantern/issues/new").expect("issue URL");
                    url.query_pairs_mut().append_pair("title",&format!("{}: ",updates::system_name())).append_pair("body",&format!("Lantern: {}\nOS: {} {}\nDesktop: {}\n\nWhat happened:\n\nSteps to reproduce:\n",env!("CARGO_PKG_VERSION"),updates::system_name(),std::env::consts::ARCH,std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()));
                    if let Err(error)=os::open(url.as_str()){self.status=format!("Could not open issue form: {error:#}");}
                }
            });ui.separator();
            if ui.button("Remove desktop integrations…").clicked(){self.uninstall_confirm=true;}
            if self.uninstall_confirm{
                ui.label("This removes Lantern's launcher, login entry, GNOME extension and agent hooks, and signs you out. Your projects, running agents, settings and speech model are kept.");
                ui.horizontal(|ui|{
                    if ui.button("Remove integrations").clicked(){self.sign_out();self.uninstall_confirm=false;os::task(self.tx.clone(),os::remove_integrations,|result|Event::Status(result.unwrap_or_else(|error|error)));}
                    if ui.button("Keep Lantern").clicked(){self.uninstall_confirm=false;}
                });
            }
        });
    }
    pub(super) fn new_session(&mut self, ctx: &egui::Context) {
        if !self.show_new {
            return;
        }
        egui::Modal::new(egui::Id::new("new-session")).show(ctx,|ui|{
            ui.set_width(550.0);ui.heading("Start something new");ui.label(RichText::new("Choose an agent and give it a place to work.").color(MUTED));
            ui.add_enabled_ui(self.launch_pending.is_none(),|ui|{
                let previous=self.kind.clone();egui::ComboBox::from_label("Agent").selected_text(&self.kind).show_ui(ui,|ui|{for kind in &self.installed{ui.selectable_value(&mut self.kind,kind.clone(),kind);}});
if previous!=self.kind{self.load_models();}
                if self.installed.is_empty(){ui.label("No supported agent found in your login-shell PATH.");}
                if self.choosable{egui::ComboBox::from_label("Model").selected_text(if self.model.is_empty(){"Agent default"}else{&self.model}).show_ui(ui,|ui|{ui.selectable_value(&mut self.model,String::new(),"Agent default");for model in &self.models{ui.selectable_value(&mut self.model,string(model,"id").into(),string(model,"label"));}});}
                ui.label("Project folder");ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut self.cwd).desired_width(425.0));
if ui.button("Browse…").clicked(){os::task(self.tx.clone(),os::folder,Event::Folder);}});
                ui.label("First message");ui.add(egui::TextEdit::multiline(&mut self.new_draft.text).hint_text("What should we work on?").desired_rows(4).desired_width(f32::INFINITY));self.screenshot_preview(ui,"__new");
                ui.horizontal(|ui|{if ui.add_enabled(!self.capture_pending,egui::Button::new("Attach screenshot")).clicked(){self.capture("__new".into());}
if ui.add_enabled(!self.speech_busy,egui::Button::new(if self.recording.is_some(){"Stop dictation"}else{"Dictate"})).clicked(){self.dictation("__new".into());}});
                ui.checkbox(&mut self.background,if cfg!(windows){"Run in the background (open it any time)"}else{"Run in the background with tmux"});ui.checkbox(&mut self.settings.toolkit,"Share local toolkit");ui.checkbox(&mut self.settings.bus,"Join October Bus");
            });
            ui.horizontal(|ui|{
                if ui.add_enabled(self.connected&&self.launch_pending.is_none()&&!self.capture_pending&&!self.kind.is_empty()&&!self.cwd.trim().is_empty(),egui::Button::new("Start session  ↗")).clicked(){
                    os::allow_focus(self.engine.pid()); self.cancel_speech(); let prompt=(!self.new_draft.text.is_empty()).then_some(&self.new_draft.text); let model=(!self.model.is_empty()).then_some(&self.model); self.launch_pending=self.request(json!({"type":"launch","kind":self.kind,"cwd":self.cwd,"prompt":prompt,"screenshot":self.new_draft.screenshot,"model":model,"background":self.background,"toolkit":self.settings.toolkit,"bus":self.settings.bus}));self.status="Starting your agent…".into();
                }
                if self.launch_pending.is_none()&&ui.button("Cancel").clicked(){self.show_new=false;self.cancel_speech();self.capture_generation+=1;}
                if self.launch_pending.is_some(){ui.spinner();}
            });
        });
    }
}
