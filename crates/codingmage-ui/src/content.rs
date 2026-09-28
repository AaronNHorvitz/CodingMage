//! Inert, bounded presentation of repository and backend text.
//!
//! Content cannot create egui widgets. Links are extracted only as candidates and require a
//! separate trusted confirmation before the native shell asks the desktop to open one.

use egui::{Context, Id, Ui};

/// Maximum visible characters from one untrusted field.
pub const MAX_PREVIEW_CHARS: usize = 4096;
/// Maximum number of candidate links offered from one field.
pub const MAX_LINKS: usize = 8;
const MAX_LINK_BYTES: usize = 2048;

/// Inert content prepared for one UI label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentPreview {
    /// Visible, control-safe text.
    pub text: String,
    /// Complete HTTPS addresses offered for separate confirmation.
    pub links: Vec<String>,
    /// Whether content exceeded the visible limit.
    pub truncated: bool,
}

impl ContentPreview {
    /// Sanitizes and bounds text before creating any widget or link affordance.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut text = String::new();
        let mut truncated = false;
        for (index, character) in source.chars().enumerate() {
            if index == MAX_PREVIEW_CHARS {
                truncated = true;
                break;
            }
            if character == '\n' || character == '\t' {
                text.push(character);
            } else if character.is_control() || is_direction_override(character) {
                text.push('�');
            } else {
                text.push(character);
            }
        }
        let mut links = Vec::new();
        for word in text.split_whitespace() {
            let candidate = word
                .trim_start_matches(['(', '[', '<', '"', '\''])
                .trim_end_matches(['.', ',', ';', '!', '?', ')', ']', '>', '"', '\'']);
            if valid_external_link(candidate) && !links.iter().any(|link| link == candidate) {
                links.push(candidate.to_owned());
                if links.len() == MAX_LINKS {
                    break;
                }
            }
        }
        Self {
            text,
            links,
            truncated,
        }
    }
}

fn is_direction_override(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

fn valid_external_link(candidate: &str) -> bool {
    let Some(rest) = candidate.strip_prefix("https://") else {
        return false;
    };
    if candidate.len() > MAX_LINK_BYTES
        || !candidate.is_ascii()
        || candidate.bytes().any(|byte| {
            !byte.is_ascii_graphic() || matches!(byte, b'\\' | b'<' | b'>' | b'"' | b'\'' | b'@')
        })
    {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let lower_host = host.to_ascii_lowercase();
    if !host.contains('.')
        || [".local", ".localhost", ".internal", ".lan"]
            .iter()
            .any(|suffix| lower_host.ends_with(suffix))
    {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) && !host
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

fn pending_link_id() -> Id {
    Id::new("codingmage.confirm-external-link")
}

/// Shows source text as inert text and offers separately labelled candidate links.
pub fn render(ui: &mut Ui, source: &str) {
    let preview = ContentPreview::new(source);
    ui.monospace(&preview.text);
    if preview.truncated {
        ui.small("Preview shortened after 4,096 characters. Narrow the source to inspect more.");
    }
    for link in preview.links {
        ui.horizontal_wrapped(|ui| {
            ui.small("External address in untrusted content:");
            ui.monospace(&link);
            if ui.button("Review external link").clicked() {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(pending_link_id(), link));
            }
        });
    }
}

/// Draws the trusted shell confirmation after every screen has rendered.
pub fn confirmation(ctx: &Context) {
    let pending = ctx.data(|data| data.get_temp::<String>(pending_link_id()));
    let Some(url) = pending.filter(|url| !url.is_empty()) else {
        return;
    };
    egui::Window::new("Open external link?")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("This address came from repository or backend content. Review the full destination before opening it in your browser.");
            egui::ScrollArea::horizontal().max_width(600.0).show(ui, |ui| {
                ui.monospace(&url);
            });
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ctx.data_mut(|data| data.insert_temp(pending_link_id(), String::new()));
                }
                if ui.button("Open in browser").clicked() {
                    if valid_external_link(&url) {
                        ctx.open_url(egui::OpenUrl::new_tab(&url));
                    }
                    ctx.data_mut(|data| data.insert_temp(pending_link_id(), String::new()));
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_markup_images_scripts_and_spoofed_controls_stay_plain_text() {
        let source = "<script>open('https://evil.example')</script> ![remote](https://img.example/a.png) <button>Admit campaign</button> \u{202e}cancel\u{001b}[31m";
        let preview = ContentPreview::new(source);
        assert!(preview.text.contains("<script>"));
        assert!(preview.text.contains("![remote]"));
        assert!(preview.text.contains("<button>Admit campaign</button>"));
        assert!(!preview.text.contains('\u{202e}'));
        assert!(!preview.text.contains('\u{001b}'));
        assert!(preview.links.is_empty());
    }

    #[test]
    fn only_complete_external_https_links_can_be_offered_for_confirmation() {
        let preview = ContentPreview::new(
            "See (https://docs.example.org/path?q=1). http://plain.example/a https://user@bad.example/a https://localhost/a https://127.0.0.1/a https://docs.example.org/path?q=1",
        );
        assert_eq!(preview.links, ["https://docs.example.org/path?q=1"]);
        assert!(!valid_external_link("https://good.example\\@bad.example"));
        assert!(!valid_external_link(
            "https://docs.example.org/\u{202e}spoof"
        ));
        assert!(!valid_external_link("https://HOST.LOCAL/path"));
    }

    #[test]
    fn oversized_content_is_bounded_before_link_extraction() {
        let source = format!(
            "{} https://later.example/path",
            "x".repeat(MAX_PREVIEW_CHARS)
        );
        let preview = ContentPreview::new(&source);
        assert_eq!(preview.text.chars().count(), MAX_PREVIEW_CHARS);
        assert!(preview.truncated);
        assert!(preview.links.is_empty());
    }
}
