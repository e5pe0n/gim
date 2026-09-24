//! Rendering.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};
use crate::config::Keys;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [title_area, list_area, status_area, help_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let mut title = vec![Span::from("gim — branches").bold()];
    if app.in_visual() {
        title.push(Span::raw(" "));
        title.push(
            Span::from(" VISUAL ")
                .bold()
                .fg(Color::Black)
                .bg(Color::Magenta),
        );
    }
    frame.render_widget(Line::from(title), title_area);

    draw_list(frame, app, list_area);

    let prompt = match app.mode {
        Mode::Rename => format!("rename {} to: ", app.input_target),
        Mode::Create => format!("new branch from {}: ", app.input_target),
        _ => String::new(),
    };
    let status = match app.mode {
        Mode::Rename | Mode::Create => Line::from(vec![
            Span::raw(prompt.as_str()),
            Span::raw(app.input.value()),
        ]),
        Mode::Confirm => {
            let verb = if app.pending_force {
                "Force delete"
            } else {
                "Delete"
            };
            Line::from(format!("{verb} {}? [y/N]", app.pending_delete.join(", "))).fg(Color::Red)
        }
        _ => match &app.status {
            Some(s) if s.error => Line::from(one_line(&s.text)).fg(Color::Red),
            Some(s) => Line::from(s.text.clone()).fg(Color::Green),
            None => Line::default(),
        },
    };
    frame.render_widget(Paragraph::new(status), status_area);
    if matches!(app.mode, Mode::Rename | Mode::Create) {
        let before: String = app.input.value().chars().take(app.input.cursor).collect();
        let x = status_area.x + (prompt.width() + before.width()) as u16;
        frame.set_cursor_position(Position::new(
            x.min(status_area.right().saturating_sub(1)),
            status_area.y,
        ));
    }

    frame.render_widget(Line::from(help(app)).fg(Color::DarkGray), help_area);
}

fn draw_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.branches.is_empty() {
        frame.render_widget(Line::from("  (no branches)").fg(Color::DarkGray), area);
        return;
    }
    let name_width = app
        .branches
        .iter()
        .map(|b| b.name.width())
        .max()
        .unwrap_or(0);
    let (lo, hi) = app.selection();
    let visual = app.in_visual();
    let full = Style::default();

    let items: Vec<ListItem> = app
        .branches
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let marker = if b.current { "* " } else { "  " };
            let name = format!("{:<name_width$}", b.name);
            let highlighted = i == app.cursor || (visual && (lo..=hi).contains(&i));
            if highlighted {
                // Plain text so the row highlight applies uniformly.
                let text = format!("{marker}{name}  {}  {}", b.hash, b.subject);
                let style = if i == app.cursor {
                    full.add_modifier(Modifier::REVERSED)
                } else {
                    full.bg(Color::Indexed(24)).fg(Color::White)
                };
                return ListItem::new(Line::from(text)).style(style);
            }
            let name_style = if b.current {
                full.fg(Color::Green).bold()
            } else {
                full
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(name, name_style),
                Span::raw("  "),
                Span::styled(b.hash.clone(), full.fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(b.subject.clone(), full.fg(Color::DarkGray)),
            ]))
        })
        .collect();

    app.list_state.select(Some(app.cursor));
    frame.render_stateful_widget(List::new(items), area, &mut app.list_state);
}

fn help(app: &App) -> String {
    match app.mode {
        Mode::Rename | Mode::Create => return "enter: confirm  esc: cancel".into(),
        Mode::Confirm => return "y: confirm  any other key: cancel".into(),
        _ => {}
    }
    let k = &app.cfg.keys;
    let mut parts: Vec<(&Keys, &str)> = vec![
        (&k.down, "down"),
        (&k.up, "up"),
        (&k.checkout, "checkout"),
        (&k.checkout_new, "checkout -b"),
        (&k.rename, "rename"),
        (&k.delete, "delete"),
        (&k.force_delete, "force delete"),
    ];
    if app.mode == Mode::Visual {
        parts.push((&k.cancel, "cancel"));
    } else {
        parts.push((&k.visual, "visual"));
    }
    parts.push((&k.quit, "quit"));
    parts
        .into_iter()
        .filter_map(|(keys, desc)| keys.primary().map(|key| format!("{key}: {desc}")))
        .collect::<Vec<_>>()
        .join("  ")
}

fn one_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}
