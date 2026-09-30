//! Rendering.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Choice, Mode};
use crate::config::Keys;
use crate::git::Op;

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
    if let Some(o) = &app.operation {
        let badge = match o.op {
            Op::Merge => " MERGING ",
            Op::Rebase => " REBASING ",
        };
        title.push(Span::raw(" "));
        title.push(Span::from(badge).bold().fg(Color::Black).bg(Color::Yellow));
        title.push(Span::raw(format!(" {}", o.desc)));
        if !o.conflicts.is_empty() {
            title.push(
                Span::from(format!(" ({})", conflict_count(o.conflicts.len()))).fg(Color::Red),
            );
        }
    }
    if app.filtering() && app.mode != Mode::Search {
        title.push(Span::from(format!("  /{}", app.filter.value())).fg(Color::Magenta));
    }
    if let Some(y) = &app.yanked {
        title.push(Span::from(format!("  yanked: {y}")).fg(Color::Cyan));
    }
    frame.render_widget(Line::from(title), title_area);

    draw_list(frame, app, list_area);

    let prompt = match app.mode {
        Mode::Rename => format!("rename {} to: ", app.input_target),
        Mode::Create => format!("new branch from {}: ", app.input_target),
        Mode::Search => "/".into(),
        _ => String::new(),
    };
    let input = if app.mode == Mode::Search {
        &app.filter
    } else {
        &app.input
    };
    let status = match app.mode {
        Mode::Rename | Mode::Create | Mode::Search => {
            Line::from(vec![Span::raw(prompt.as_str()), Span::raw(input.value())])
        }
        Mode::Confirm => {
            let verb = if app.pending_force {
                "Force delete"
            } else {
                "Delete"
            };
            Line::from(format!("{verb} {}? [y/N]", app.pending_delete.join(", "))).fg(Color::Red)
        }
        Mode::Operation => operation_line(app),
        _ => match &app.status {
            Some(s) if s.error => Line::from(one_line(&s.text)).fg(Color::Red),
            Some(s) => Line::from(s.text.clone()).fg(Color::Green),
            None => Line::default(),
        },
    };
    frame.render_widget(Paragraph::new(status), status_area);
    if matches!(app.mode, Mode::Rename | Mode::Create | Mode::Search) {
        let before: String = input.value().chars().take(input.cursor).collect();
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
        let text = if app.filtering() {
            "  (no matching branches)"
        } else {
            "  (no branches)"
        };
        frame.render_widget(Line::from(text).fg(Color::DarkGray), area);
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
            let pad = " ".repeat(name_width - b.name.width());
            let positions = app.matches.get(i).map(Vec::as_slice).unwrap_or_default();
            let highlighted = i == app.cursor || (visual && (lo..=hi).contains(&i));
            if highlighted {
                // Unstyled apart from matches so the row highlight applies uniformly.
                let mut spans = vec![Span::raw(marker)];
                spans.extend(name_spans(
                    &b.name,
                    positions,
                    full,
                    full.bold().underlined(),
                ));
                spans.push(Span::raw(format!("{pad}  {}  {}", b.hash, b.subject)));
                let style = if i == app.cursor {
                    full.add_modifier(Modifier::REVERSED)
                } else {
                    full.bg(Color::Indexed(24)).fg(Color::White)
                };
                return ListItem::new(Line::from(spans)).style(style);
            }
            let name_style = if b.current {
                full.fg(Color::Green).bold()
            } else {
                full
            };
            let mut spans = vec![Span::raw(marker)];
            spans.extend(name_spans(
                &b.name,
                positions,
                name_style,
                full.fg(Color::Magenta).bold(),
            ));
            spans.extend([
                Span::raw(pad),
                Span::raw("  "),
                Span::styled(b.hash.clone(), full.fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(b.subject.clone(), full.fg(Color::DarkGray)),
            ]);
            ListItem::new(Line::from(spans))
        })
        .collect();

    app.list_state.select(Some(app.cursor));
    frame.render_stateful_widget(List::new(items), area, &mut app.list_state);
}

/// `name` split into runs, with the chars at `positions` in `matched` style.
fn name_spans(name: &str, positions: &[usize], plain: Style, matched: Style) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = Vec::new();
    let mut run = String::new();
    let mut run_matched = false;
    for (i, c) in name.chars().enumerate() {
        let m = positions.contains(&i);
        if m != run_matched && !run.is_empty() {
            let style = if run_matched { matched } else { plain };
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        run_matched = m;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, if run_matched { matched } else { plain }));
    }
    spans
}

fn conflict_count(n: usize) -> String {
    format!("{n} conflicted file{}", if n == 1 { "" } else { "s" })
}

fn operation_line(app: &App) -> Line<'static> {
    let Some(o) = &app.operation else {
        return Line::default();
    };
    let state = if o.conflicts.is_empty() {
        "no conflicts left".to_string()
    } else {
        conflict_count(o.conflicts.len())
    };
    let mut spans =
        vec![
            Span::from(format!("{}, {state}: ", o.desc)).fg(if o.conflicts.is_empty() {
                Color::Green
            } else {
                Color::Red
            }),
        ];
    for (i, choice) in Choice::ALL.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let span = Span::from(format!(" {} ", choice.label()));
        spans.push(if app.choice == choice {
            span.bold().fg(Color::Black).bg(Color::Yellow)
        } else {
            span
        });
    }
    Line::from(spans)
}

fn help(app: &App) -> String {
    match app.mode {
        Mode::Rename | Mode::Create => return "enter: confirm  esc: cancel".into(),
        Mode::Confirm => return "y: confirm  any other key: cancel".into(),
        Mode::Search => {
            return "type to filter  up/down: move  enter: done  esc: clear".into();
        }
        Mode::Operation => {
            return "h/l: choose  enter: select  c: continue  r: resolve  a: abort  esc: back"
                .into();
        }
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
        (&k.yank, "yank"),
        (&k.search, "search"),
    ];
    if app.operation.is_some() {
        parts.push((&k.operation, "continue/abort"));
    } else if app.yanked.is_some() {
        parts.push((&k.merge, "merge yanked"));
        parts.push((&k.rebase, "rebase yanked"));
    }
    if app.mode == Mode::Visual {
        parts.push((&k.cancel, "cancel"));
    } else {
        parts.push((&k.visual, "visual"));
        if app.filtering() {
            parts.push((&k.cancel, "clear filter"));
        }
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
