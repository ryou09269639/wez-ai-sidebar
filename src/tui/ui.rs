use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::{
    config::SidebarConfig,
    state::{AgentState, AgentStatus},
};

pub fn render(
    frame: &mut Frame<'_>,
    agents: &[AgentState],
    selected: usize,
    help: bool,
    config: &SidebarConfig,
) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).areas(frame.area());
    let mut lines = Vec::new();
    if agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "No agents",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from("Run doctor or start an agent."));
    } else {
        for (index, agent) in agents.iter().enumerate() {
            let selected_style = if index == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let marker = if index == selected { ">" } else { " " };
            let bullet = if config.unicode { "●" } else { "*" };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), selected_style),
                Span::styled(
                    format!("{bullet} "),
                    Style::default().fg(status_color(agent.status)),
                ),
                Span::styled(agent.agent.display_name(), selected_style),
            ]));
            if config.show_cwd {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        truncate(
                            &agent.project,
                            frame.area().width.saturating_sub(5) as usize,
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            let label = agent
                .permission
                .map(|permission| permission.label())
                .unwrap_or_else(|| agent.status.label());
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    label,
                    Style::default()
                        .fg(status_color(agent.status))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            if config.show_message {
                if let Some(message) = &agent.message {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            truncate(message, frame.area().width.saturating_sub(5) as usize),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }
            lines.push(Line::default());
        }
    }
    let title = if help {
        " AI AGENTS · HELP "
    } else {
        " AI AGENTS "
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::default().title(title).borders(Borders::RIGHT))
            .wrap(Wrap { trim: true }),
        body,
    );
    let footer_text = if help {
        "j/k move · Enter focus · 1-9 jump · r rescan · q close"
    } else {
        "? help · Enter focus"
    };
    frame.render_widget(
        Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn status_color(status: AgentStatus) -> Color {
    match status {
        AgentStatus::PermissionRequired | AgentStatus::WaitingInput | AgentStatus::Error => {
            Color::Red
        }
        AgentStatus::Working => Color::Yellow,
        AgentStatus::Idle => Color::Green,
        AgentStatus::Done => Color::Blue,
        AgentStatus::Unknown => Color::DarkGray,
    }
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = value.chars().count();
    if count <= width {
        return value.to_owned();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", value.chars().take(width - 1).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_character_not_byte() {
        assert_eq!(truncate("日本語test", 4), "日本語…");
    }
}
