//! Exact, display-only command previews for actions sent to the sibling coordinator.

use std::path::Path;

use egui::Ui;

/// Formats the exact UTF-8 argument vector as a POSIX shell command.
///
/// Non-printable or direction-control arguments are refused because a visually deceptive
/// command cannot satisfy the native interface's "Show command" promise.
#[must_use]
pub fn format_command(binary: &Path, arguments: &[String]) -> Option<String> {
    let executable = binary.to_str()?;
    std::iter::once(executable)
        .chain(arguments.iter().map(String::as_str))
        .map(quote_argument)
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(" "))
}

fn quote_argument(argument: &str) -> Option<String> {
    if !argument.chars().all(display_safe_character) {
        return None;
    }
    if argument.is_empty() {
        return Some("''".to_owned());
    }
    if argument
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return Some(argument.to_owned());
    }
    Some(format!("'{}'", argument.replace('\'', "'\\''")))
}

// A command must remain legible without depending on the current font's treatment of
// format characters, combining marks, unusual whitespace or invisible symbols. Admit
// ordinary non-ASCII letters and numbers for international paths, except blank letters.
fn display_safe_character(character: char) -> bool {
    if character.is_ascii() {
        return character == ' ' || character.is_ascii_graphic();
    }
    character.is_alphanumeric()
        && !matches!(character, '\u{115f}' | '\u{1160}' | '\u{3164}' | '\u{ffa0}')
}

/// Whether the exact command can be rendered for an enabled action.
#[must_use]
pub fn can_preview(binary: Option<&Path>, arguments: Option<&[String]>) -> bool {
    binary
        .zip(arguments)
        .is_some_and(|(path, args)| format_command(path, args).is_some())
}

/// Shows the exact command next to the matching UI action, without executing or copying it.
pub fn show(ui: &mut Ui, binary: Option<&Path>, arguments: &[String]) {
    ui.collapsing("Show command", |ui| {
        match binary.and_then(|path| format_command(path, arguments)) {
            Some(command) => {
                ui.monospace(command);
                ui.small("This is the command the app sends when you choose the action. Showing it does not run it.");
            }
            None => {
                ui.label("No exact command is available until the coordinator and required inputs are selected, or a path contains an invisible control character.");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_preserves_argument_boundaries_and_rejects_spoofed_display() {
        let arguments = vec![
            "doctor".to_owned(),
            "--config".to_owned(),
            "/tmp/a b/it'works.toml".to_owned(),
        ];
        assert_eq!(
            format_command(Path::new("/usr/bin/codingmage"), &arguments),
            Some("/usr/bin/codingmage doctor --config '/tmp/a b/it'\\''works.toml'".to_owned())
        );
        assert!(format_command(Path::new("/tmp/\u{202e}codex"), &arguments).is_none());
        assert!(
            format_command(
                Path::new("/usr/bin/codingmage"),
                &["in\u{200b}visible".to_owned()]
            )
            .is_none()
        );
        assert!(format_command(Path::new("/usr/bin/codingmage"), &["a\nb".to_owned()]).is_none());
        assert!(format_command(Path::new("/tmp/a\u{034f}b/codingmage"), &arguments).is_none());
        assert!(format_command(Path::new("/tmp/a\u{180e}b/codingmage"), &arguments).is_none());
        assert!(format_command(Path::new("/tmp/a\u{115f}b/codingmage"), &arguments).is_none());
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{034f}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{180e}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{2800}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{00a0}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{fe0f}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{200d}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{202e}b".to_owned()]).is_none()
        );
        assert!(
            format_command(Path::new("/usr/bin/codingmage"), &["a\u{3164}b".to_owned()]).is_none()
        );
        assert!(format_command(Path::new("/usr/bin/codingmage"), &["日本語".to_owned()]).is_some());
        let hidden = Path::new("/tmp/a\u{034f}b/codingmage");
        assert!(!can_preview(Some(hidden), Some(&arguments)));
        assert!(!can_preview(
            Some(hidden),
            Some(&["campaign-preflight".to_owned()])
        ));
        assert_eq!(quote_argument(""), Some("''".to_owned()));
    }
}
