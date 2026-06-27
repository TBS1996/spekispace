use std::{cell::RefCell, rc::Rc, sync::Arc};

use dioxus::prelude::*;
use either::Either;
use serde::Deserialize;
use speki_core::card::TextData;

use crate::{components::set_card_link, overlays::OverlayEnum, APP};

const LINK_EDITOR_CSS: &str = r#"
.link-editor { white-space: pre-wrap; word-break: break-word; }
.link-editor:empty:before {
    content: attr(data-placeholder);
    color: #9ca3af;
    pointer-events: none;
}
.link-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    background: #dbeafe;
    color: #1d4ed8;
    border-radius: 0.375rem;
    padding: 0 0.35rem;
    margin: 0 1px;
    font-weight: 500;
    user-select: none;
}
.link-chip-name { cursor: pointer; }
.link-chip-remove {
    cursor: pointer;
    color: #1d4ed8;
    opacity: 0.55;
    font-weight: 700;
    line-height: 1;
}
.link-chip-remove:hover { opacity: 1; }
"#;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum EditorMsg {
    #[serde(rename = "sync")]
    Sync { raw: String },
    #[serde(rename = "edit")]
    Edit { id: String, alias: String },
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;")
}

/// Build the inner HTML for the editor: plain text segments stay as text,
/// card links become non-editable "chips" showing the card's name (or alias).
fn render_editor_html(raw: &str) -> String {
    let app = APP.read();
    let td = TextData::from_raw(raw);
    let mut out = String::new();

    for cmp in td.inner() {
        match cmp {
            Either::Left(s) => out.push_str(&escape_html(s)),
            Either::Right(link) => {
                let id = link.id.to_string();
                let default_name = app
                    .try_load_card(link.id)
                    .map(|c| c.name().to_string())
                    .unwrap_or_else(|| "⟨missing card⟩".to_string());
                let display = link.alias.clone().unwrap_or_else(|| default_name.clone());

                out.push_str(&format!(
                    "<span class=\"link-chip\" contenteditable=\"false\" data-link-id=\"{id}\" data-default=\"{def}\"><span class=\"link-chip-name\">{name}</span><span class=\"link-chip-remove\">×</span></span>",
                    id = escape_attr(&id),
                    def = escape_attr(&default_name),
                    name = escape_html(&display),
                ));
            }
        }
    }

    out
}

/// Set (or clear, when `alias` is `None`) the alias of every link pointing at `id`.
fn set_link_alias(raw: &str, id: &str, alias: Option<String>) -> String {
    let mut td = TextData::from_raw(raw);
    for cmp in td.inner_mut() {
        if let Either::Right(link) = cmp {
            if link.id.to_string() == id {
                link.alias = alias.clone();
            }
        }
    }
    td.to_raw()
}

/// A rich text editor for card text that renders `[[guid]]` references as
/// readable, removable chips instead of raw guids. The underlying `text`
/// signal still holds the raw `[[guid|alias]]` form, so this is a drop-in
/// replacement for a plain `<input>` bound to the same signal.
#[component]
pub fn LinkEditor(
    text: Signal<String>,
    placeholder: String,
    #[props(default = false)] disabled: bool,
) -> Element {
    let id = use_hook(|| format!("link-editor-{}", uuid::Uuid::new_v4()));

    // `dom_html` only seeds the *initial* paint of the contenteditable. We never
    // change it afterwards, because routing live updates through Dioxus's
    // innerHTML diffing is unreliable here: the DOM is mutated directly by the
    // browser as the user types (so Dioxus's tracked value goes stale), which
    // makes it skip a later update whose value happens to match the stale one.
    // Instead, every programmatic change is written straight to the DOM via a
    // one-shot JS `innerHTML` assignment from the body below.
    let dom_html = use_signal(|| render_editor_html(&text.peek()));

    // `last_sync` is the raw text currently matching the DOM, used to tell a
    // programmatic change apart from one the user typed.
    let last_sync = use_hook(|| Rc::new(RefCell::new(text.peek().clone())));

    // `latest` always points at the *current* `text` signal. Persistent hooks
    // (the listener loop, alias hook) capture this Rc rather than the signal
    // directly, so when the parent swaps in a different card's signal without
    // remounting us, write-backs still target the signal currently on screen.
    let latest = use_hook(|| Rc::new(RefCell::new(text)));

    // Read `text()` here in the body so we re-subscribe to the *current* signal
    // on every render. This is what makes navigation between cards work even
    // when this component is reused (not remounted): the body re-runs with the
    // new signal, and we push its content to the DOM.
    {
        *latest.borrow_mut() = text;
        let current = text();
        if current != *last_sync.borrow() {
            *last_sync.borrow_mut() = current.clone();
            let id = id.clone();
            spawn(async move {
                let html = render_editor_html(&current);
                let payload =
                    serde_json::to_string(&html).unwrap_or_else(|_| "\"\"".to_string());
                let script = format!(
                    r#"const el = document.getElementById("{id}"); if (el) {{ el.innerHTML = {payload}; }}"#
                );
                document::eval(&script);
            });
        }
    }

    // Attach DOM listeners once, after mount.
    {
        let id = id.clone();
        let last_sync = last_sync.clone();
        let latest = latest.clone();
        use_effect(move || {
            let last_sync = last_sync.clone();
            let latest = latest.clone();
            let script = format!(
                r#"
                const el = document.getElementById("{id}");
                if (el && !el.dataset.linkEditorReady) {{
                    el.dataset.linkEditorReady = "1";
                    const disabled = {disabled};

                    const serialize = () => {{
                        let out = "";
                        el.childNodes.forEach((node) => {{
                            if (node.nodeType === 3) {{
                                out += node.textContent;
                            }} else if (node.nodeType === 1) {{
                                if (node.dataset && node.dataset.linkId) {{
                                    const lid = node.dataset.linkId;
                                    const def = node.dataset.default || "";
                                    const nameEl = node.querySelector(".link-chip-name");
                                    const txt = nameEl ? nameEl.textContent : "";
                                    if (txt === def) {{
                                        out += "[[" + lid + "]]";
                                    }} else {{
                                        out += "[[" + lid + "|" + txt + "]]";
                                    }}
                                }} else {{
                                    out += node.textContent;
                                }}
                            }}
                        }});
                        return out;
                    }};

                    el.addEventListener("input", () => {{
                        dioxus.send({{ kind: "sync", raw: serialize() }});
                    }});

                    if (!disabled) {{
                        el.addEventListener("click", (ev) => {{
                            const rm = ev.target.closest(".link-chip-remove");
                            if (rm) {{
                                const chip = rm.closest(".link-chip");
                                if (chip) {{
                                    chip.remove();
                                    dioxus.send({{ kind: "sync", raw: serialize() }});
                                }}
                                ev.preventDefault();
                                return;
                            }}
                            const chip = ev.target.closest(".link-chip");
                            if (chip) {{
                                const nameEl = chip.querySelector(".link-chip-name");
                                dioxus.send({{
                                    kind: "edit",
                                    id: chip.dataset.linkId,
                                    alias: nameEl ? nameEl.textContent : "",
                                }});
                                ev.preventDefault();
                            }}
                        }});
                    }}
                }}
                "#
            );

            let mut eval = document::eval(&script);
            spawn(async move {
                loop {
                    match eval.recv::<EditorMsg>().await {
                        Ok(EditorMsg::Sync { raw }) => {
                            *last_sync.borrow_mut() = raw.clone();
                            let mut text = *latest.borrow();
                            text.set(raw);
                        }
                        Ok(EditorMsg::Edit { id: link_id, alias }) => {
                            let latest = latest.clone();
                            let hook: Arc<Box<dyn Fn(String)>> = Arc::new(Box::new(move |s: String| {
                                let mut text = *latest.borrow();
                                let alias = {
                                    let trimmed = s.trim();
                                    if trimmed.is_empty() {
                                        None
                                    } else {
                                        Some(trimmed.to_string())
                                    }
                                };
                                let new_raw =
                                    set_link_alias(&text.peek(), &link_id, alias);
                                text.set(new_raw);
                            }));

                            OverlayEnum::new_text_input(
                                format!("alias for \"{alias}\" (empty = use card name)"),
                                hook,
                            )
                            .append();
                        }
                        Err(_) => break,
                    }
                }
            });
        });
    }

    rsx! {
        style { dangerous_inner_html: LINK_EDITOR_CSS }
        div {
            id: "{id}",
            class: "link-editor bg-white w-full border border-gray-300 rounded-md p-2 text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent min-h-[2.6rem]",
            contenteditable: if disabled { "false" } else { "true" },
            "data-placeholder": "{placeholder}",
            dangerous_inner_html: "{dom_html}",
            onkeydown: move |e| {
                if e.key() == Key::Enter {
                    e.prevent_default();
                }
            },
            onmouseup: move |e| {
                if !disabled {
                    set_card_link(text, e.modifiers().shift(), e.modifiers().alt());
                }
            },
        }
    }
}
