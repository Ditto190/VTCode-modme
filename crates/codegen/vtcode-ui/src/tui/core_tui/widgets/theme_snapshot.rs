use std::sync::Arc;

use anstyle::{AnsiColor, Color as AnsiColorEnum, Style as AnsiStyle};
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend, prelude::Color, widgets::Widget};
use vtcode_commons::ui_protocol::{InlineMessageKind, InlineSegment, InlineTextStyle, InlineTheme};

use super::SessionWidget;
use crate::tui::core_tui::session::Session;
use crate::tui::ui::shell_syntax::{ShellLineStyles, shell_syntax_segments_with_highlighted};

fn status_segment(color: AnsiColor) -> InlineSegment {
    InlineSegment {
        text: "• ".to_string(),
        style: Arc::new(InlineTextStyle {
            color: Some(AnsiColorEnum::Ansi(color)),
            ..InlineTextStyle::default()
        }),
    }
}

fn color_name(color: Option<AnsiColorEnum>) -> String {
    match color {
        Some(AnsiColorEnum::Ansi(color)) => format!("{color:?}"),
        Some(AnsiColorEnum::Ansi256(color)) => format!("Ansi256({})", color.0),
        Some(AnsiColorEnum::Rgb(color)) => format!("Rgb({}, {}, {})", color.0, color.1, color.2),
        None => "None".to_string(),
    }
}

fn rendered_bullet_colors() -> Vec<Color> {
    let theme = InlineTheme {
        foreground: Some(AnsiColorEnum::Ansi(AnsiColor::White)),
        background: Some(AnsiColorEnum::Ansi(AnsiColor::Black)),
        ..InlineTheme::default()
    };
    let mut session = Session::new(theme, None, 8);
    for (status, color) in [
        ("success", AnsiColor::Green),
        ("failure", AnsiColor::Red),
        ("warning", AnsiColor::Yellow),
    ] {
        session.push_line(
            InlineMessageKind::Tool,
            vec![
                status_segment(color),
                InlineSegment {
                    text: format!("{status} command"),
                    style: Arc::new(InlineTextStyle::default()),
                },
            ],
        );
    }

    let backend = TestBackend::new(48, 12);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            let mut widget = SessionWidget::new(&mut session);
            (&mut widget).render(frame.area(), frame.buffer_mut());
        })
        .expect("render status bullets");

    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "•")
        .map(|cell| cell.fg)
        .collect()
}

#[test]
fn tool_status_bullets_and_uniform_shell_fallback() {
    let bullet_colors = rendered_bullet_colors();
    assert_eq!(bullet_colors, vec![Color::Green, Color::Red, Color::Yellow]);

    let styles = ShellLineStyles::new();
    let uniform_style = AnsiStyle::new().fg_color(Some(AnsiColorEnum::Ansi(AnsiColor::Cyan)));
    let highlighted = ["cargo", " ", "check", " ", "-p", " ", "vtcode"]
        .into_iter()
        .map(|text| (uniform_style, text.to_string()))
        .collect();
    let fallback = shell_syntax_segments_with_highlighted("cargo check -p vtcode", &styles, true, Some(highlighted));
    let command = fallback.iter().find(|segment| segment.text == "cargo").expect("command token");
    let option = fallback.iter().find(|segment| segment.text == "-p").expect("option token");

    assert_eq!(command.style.color, styles.command.color);
    assert_eq!(option.style.color, styles.option.color);
    assert_ne!(command.style.color, option.style.color);

    let rendered = format!(
        "success • prefix: {:?}\nfailure • prefix: {:?}\nwarning • prefix: {:?}\nuniform shell highlight fallback:\n  command cargo: {}\n  option -p: {}\n  semantic colors distinct: {}",
        bullet_colors[0],
        bullet_colors[1],
        bullet_colors[2],
        color_name(command.style.color),
        color_name(option.style.color),
        command.style.color != option.style.color,
    );
    assert_snapshot!(rendered);
}
