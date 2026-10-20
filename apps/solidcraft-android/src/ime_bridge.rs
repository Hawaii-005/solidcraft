// Builder-generated Android soft-keyboard (IME) bridge (shared module), with the snapshot delta
// fix described below.
//
// A bare NativeActivity has no editable view, so Android's InputMethodManager cannot reliably show
// the soft keyboard and never delivers typed text. The generated Activity (see
// templates/android/SafActivity.java) installs a tiny, invisible but focusable `EditText` whose real
// InputConnection fixes both: the shell calls `imeShow`/`imeHide` through the vendored
// `android-activity`, and this module drains the queue once per frame into the egui events the
// focused `TextEdit` understands. The upstream ArtCraft crates are never modified.
//
// ARTCRAFT_ANDROID_IME_BRIDGE_V2_SNAPSHOT
pub mod ime {
    use android_activity::AndroidApp;
    use jni::{
        JavaVM, jni_sig, jni_str,
        objects::{JObject, JString},
    };
    use std::sync::Arc;

    /// Emitted at startup so a device log (and the packaged `.so` strings) can confirm this shell —
    /// including the soft-keyboard bridge — is the one running.
    pub const IME_MARKER: &str = "ARTCRAFT_ANDROID_IME_BRIDGE_V2_SNAPSHOT";

    pub struct ImeBridge {
        app: AndroidApp,
        vm: Arc<JavaVM>,
        /// Set once the Activity reports it has no `imePoll` (an Activity that never adopted the IME
        /// host view), so the per-frame poll stops paying for a call that can only fail.
        unsupported: bool,
        /// The text the Activity's IME buffer handed egui last. Every event carries the whole
        /// buffer, so the change to apply is the difference from this.
        delivered: String,
    }

    impl ImeBridge {
        pub fn new(app: &AndroidApp) -> Self {
            // SAFETY: AndroidApp owns the live VM pointer for the lifetime of the activity.
            let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) };
            Self {
                app: app.clone(),
                vm: Arc::new(vm),
                unsupported: false,
                delivered: String::new(),
            }
        }

        /// Forget what egui was told. Called when the focused field changes (or the keyboard is
        /// dismissed), because the next snapshot belongs to a different buffer and the difference
        /// from the old one would delete characters from the wrong field.
        pub fn reset(&mut self) {
            self.delivered.clear();
        }

        /// Translate every keyboard event the Activity has queued since the last frame into the egui
        /// IME events the focused `TextEdit` understands. Call once per frame from `raw_input_hook`.
        pub fn poll(&mut self, raw: &mut egui::RawInput) {
            if self.unsupported {
                return;
            }
            loop {
                match self.next_event() {
                    Ok(Some(event)) => {
                        for event in translate(&event, &mut self.delivered) {
                            raw.events.push(event);
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        log::warn!("Android soft-keyboard bridge unavailable: {error}");
                        self.unsupported = true;
                        break;
                    }
                }
            }
        }

        fn next_event(&self) -> Result<Option<String>, String> {
            self.vm
                .attach_current_thread(|env| -> jni::errors::Result<Option<String>> {
                    let activity_raw = self.app.activity_as_ptr() as jni::sys::jobject;
                    // SAFETY: AndroidApp retains this global Activity reference; as_cast_raw borrows
                    // it without taking ownership or attempting to delete the reference.
                    let activity = unsafe { env.as_cast_raw::<JObject>(&activity_raw)? };
                    // An Activity that never adopted the IME host has no `imePoll`; invoking it raises
                    // a pending `NoSuchMethodError`. jni deliberately leaves that exception for the
                    // caller to handle, and this thread is shared with the SAF bridge, so clear it
                    // before reporting the missing method (otherwise the next unrelated JNI call sees
                    // a stale pending exception).
                    let value = match env.call_method(
                        &*activity,
                        jni_str!("imePoll"),
                        jni_sig!("()Ljava/lang/String;"),
                        &[],
                    ) {
                        Ok(value) => value.l()?,
                        Err(error) => {
                            env.exception_clear();
                            return Err(error);
                        }
                    };
                    if value.is_null() {
                        return Ok(None);
                    }
                    let value = JString::cast_local(env, value)?;
                    let value: String = value.try_to_string(env)?;
                    Ok(Some(value))
                })
                .map_err(|error| error.to_string())
        }
    }

    /// A synthetic Enter press. Both the IME's action key (`a`) and its Enter/newline key (a bare
    /// newline in the buffer) become this, so egui decides what Enter means for the focused field.
    fn enter_key() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// Byte length of the prefix two strings share.
    fn common_prefix(a: &str, b: &str) -> usize {
        a.chars()
            .zip(b.chars())
            .take_while(|(x, y)| x == y)
            .map(|(c, _)| c.len_utf8())
            .sum()
    }

    /// Decode one queued event. The first character tags it and the rest is the payload:
    /// `r` the whole IME buffer after a change, `a` an editor action (Done/Go/Search).
    ///
    /// `delivered` is what egui already shows from the IME, so a buffer snapshot becomes the
    /// delete-then-commit pair egui applies:
    ///
    /// - an append (`q` -> `qw`) is a commit of the added tail;
    /// - a backspace (`qw` -> `q`) is `DeleteSurrounding` of the dropped tail;
    /// - an autocorrect (`qwe` -> `que`) is a delete of the old tail plus a commit of the new one.
    ///
    /// No `Preedit` is ever emitted on purpose: egui's `TextEdit` deletes a pre-edit by the range it
    /// remembers and silently drops a `DeleteSurrounding` while a composition is open, which is
    /// exactly the state a command field is in after the first character — so ⌫ did nothing.
    fn translate(event: &str, delivered: &mut String) -> Vec<egui::Event> {
        let mut chars = event.chars();
        let Some(tag) = chars.next() else {
            return Vec::new();
        };
        let payload = chars.as_str();
        match tag {
            'r' => {
                if payload == delivered {
                    return Vec::new();
                }
                // Enter arrives as a newline in the buffer; egui ignores `Commit("\n")`.
                if payload.contains('\n') || payload.contains('\r') {
                    delivered.clear();
                    return vec![enter_key()];
                }
                let old = std::mem::take(delivered);
                let prefix = common_prefix(&old, payload);
                let removed = old[prefix..].chars().count();
                let mut events = Vec::new();
                if removed > 0 {
                    events.push(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                        before_chars: removed,
                        after_chars: 0,
                    }));
                }
                let added = &payload[prefix..];
                if !added.is_empty() {
                    events.push(egui::Event::Ime(egui::ImeEvent::Commit(added.to_owned())));
                }
                *delivered = payload.to_owned();
                log::debug!(
                    "ime: buffer {} chars, {} committed, {} deleted",
                    delivered.chars().count(),
                    added.chars().count(),
                    removed
                );
                events
            }
            // An editor action (Done/Go/Search/Send) confirms the field. IME_ACTION_NONE (0)
            // carries no meaning.
            'a' => match payload.trim().parse::<i32>() {
                Ok(action) if action != 0 => {
                    delivered.clear();
                    vec![enter_key()]
                }
                _ => Vec::new(),
            },
            // Older Activities (pre-snapshot) send pre-edit/commit/delete events one by one. Keep
            // them working rather than going silent on an app/Activity version mismatch.
            'c' if !payload.is_empty() => {
                if payload == "\n" || payload == "\r" || payload == "\r\n" {
                    delivered.clear();
                    return vec![enter_key()];
                }
                delivered.push_str(payload);
                vec![egui::Event::Ime(egui::ImeEvent::Commit(payload.to_owned()))]
            }
            'd' => {
                let Some((before, after)) = payload.split_once(',') else {
                    return Vec::new();
                };
                let before_chars = before.trim().parse().unwrap_or(0);
                let after_chars = after.trim().parse().unwrap_or(0);
                if before_chars + after_chars == 0 {
                    return Vec::new();
                }
                for _ in 0..before_chars {
                    delivered.pop();
                }
                vec![egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                    before_chars,
                    after_chars,
                })]
            }
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn committed(events: &[egui::Event]) -> String {
            let mut out = String::new();
            for event in events {
                if let egui::Event::Ime(egui::ImeEvent::Commit(text)) = event {
                    out.push_str(text);
                }
            }
            out
        }

        #[test]
        fn an_append_is_only_the_added_tail() {
            let mut delivered = String::new();
            assert_eq!(committed(&translate("rq", &mut delivered)), "q");
            assert_eq!(committed(&translate("rqw", &mut delivered)), "w");
            assert_eq!(committed(&translate("rqwe", &mut delivered)), "e");
            assert!(translate("rqwe", &mut delivered).is_empty(), "unchanged snapshot is silent");
        }

        #[test]
        fn a_backspace_deletes_the_dropped_tail() {
            let mut delivered = "qwe".to_owned();
            let events = translate("rqw", &mut delivered);
            assert_eq!(events.len(), 1);
            match &events[0] {
                egui::Event::Ime(egui::ImeEvent::DeleteSurrounding { before_chars, after_chars }) => {
                    assert_eq!((*before_chars, *after_chars), (1, 0));
                }
                other => panic!("expected a delete, got {other:?}"),
            }
            assert_eq!(delivered, "qw");
        }

        #[test]
        fn an_autocorrect_replaces_the_tail() {
            let mut delivered = "qwe".to_owned();
            let events = translate("rque", &mut delivered);
            assert_eq!(committed(&events), "que");
            assert!(matches!(
                events[0],
                egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                    before_chars: 3,
                    after_chars: 0
                })
            ));
        }

        #[test]
        fn a_newline_is_enter_and_clears_the_buffer() {
            let mut delivered = "sketch".to_owned();
            let events = translate("rsketch\n", &mut delivered);
            assert!(matches!(
                events.as_slice(),
                [egui::Event::Key { key: egui::Key::Enter, .. }]
            ));
            assert!(delivered.is_empty());
        }

        #[test]
        fn accented_characters_take_the_right_byte_prefix() {
            let mut delivered = String::new();
            assert_eq!(committed(&translate("rração", &mut delivered)), "ração");
            assert_eq!(committed(&translate("rraçõ", &mut delivered)), "");
            assert_eq!(delivered, "raçõ");
        }
    }
}
