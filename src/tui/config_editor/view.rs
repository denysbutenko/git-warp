use super::model::{ConfigEditorModel, ConfigStatusKind};
use super::spec::ConfigFieldKind;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub fn draw_config_editor(f: &mut Frame, model: &ConfigEditorModel, pending_quit: bool) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(f.area());

    let dirty_tag = if model.is_dirty() { " [unsaved]" } else { "" };
    let header_text = format!(
        "Git-Warp config editor — {}{}",
        model.config_path().display(),
        dirty_tag
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, outer[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(outer[1]);

    let section_items: Vec<ListItem> = model
        .sections
        .iter()
        .enumerate()
        .map(|(idx, section)| {
            let marker = if idx == model.section_idx() {
                "▸ "
            } else {
                "  "
            };
            let line = format!("{}{}", marker, section.label());
            let style = if idx == model.section_idx() {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(line).style(style)
        })
        .collect();
    let sections = List::new(section_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Sections (Tab)"),
    );
    f.render_widget(sections, body[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(body[1]);

    let current_section = model.current_section();
    let fields = model.fields_in_section(current_section);
    let cursor = model
        .field_idx_in_section
        .min(fields.len().saturating_sub(1));
    let field_items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let value = if model.editing() && idx == cursor {
                let buf = model.edit_buffer_value().unwrap_or("");
                format!("> {} = {}_", field.label, buf)
            } else {
                let cursor_mark = if idx == cursor { "▸" } else { " " };
                format!(
                    "{} {} = {}",
                    cursor_mark,
                    field.label,
                    model.field_value_display(field)
                )
            };
            let style = if idx == cursor {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(value).style(style)
        })
        .collect();
    let fields_block = List::new(field_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(current_section.label()),
    );
    f.render_widget(fields_block, right[0]);

    let current_field = fields[cursor];
    let mut help_lines = vec![Line::from(current_field.help.to_string())];
    if let ConfigFieldKind::Choice { allowed } = current_field.kind {
        help_lines.push(Line::from(format!("Allowed: {}", allowed.join(", "))));
    }
    if let ConfigFieldKind::U64 { min, max } = current_field.kind {
        help_lines.push(Line::from(format!("Range: {min}..={max}")));
    }
    if let ConfigFieldKind::Usize { min, max } = current_field.kind {
        help_lines.push(Line::from(format!("Range: {min}..={max}")));
    }
    let help = Paragraph::new(help_lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, right[1]);

    let status_text = if pending_quit {
        "Discard unsaved changes? y/N".to_string()
    } else if let Some(status) = model.status() {
        status.text.clone()
    } else if !model.env_overrides().is_empty() {
        format!(
            "Heads up: {} GIT_WARP_* env var(s) set — they shadow saved values at runtime.",
            model.env_overrides().len()
        )
    } else {
        String::new()
    };
    let status_color = if pending_quit {
        Color::Yellow
    } else if let Some(status) = model.status() {
        match status.kind {
            ConfigStatusKind::Success => Color::Green,
            ConfigStatusKind::Error => Color::Red,
            ConfigStatusKind::Info => Color::Cyan,
        }
    } else {
        Color::Gray
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, outer[2]);

    let footer_text = if pending_quit {
        "y discard and quit  n keep editing".to_string()
    } else if model.editing() {
        "Enter save field  Esc cancel  Backspace delete".to_string()
    } else {
        "↑↓/jk move  Tab section  Space toggle  Enter/e edit  s save  r revert  q quit".to_string()
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(footer, outer[3]);
}
