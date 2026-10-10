//! 整应用皮肤选择。缩略图用实际色板绘制，不使用静态截图。
use super::*;
use crate::theme::{ShellPalette, ShellTokens};
use crate::ui::workbench::{centered_label, separator};

pub(super) fn choices(
    ui: &mut egui::Ui,
    theme: &ThemeSettings,
    skins: &SkinCatalog,
    outbox: &mut Vec<Message>,
) {
    let dark = ui.visuals().dark_mode;
    let columns = ((ui.available_width() + 10.0) / 112.0)
        .floor()
        .clamp(1.0, 4.0) as usize;
    let width = (ui.available_width() - (columns - 1) as f32 * 10.0) / columns as f32;
    let choices: Vec<_> = std::iter::once((None, ShellPalette::default()))
        .chain(skins.skins.iter().map(|skin| {
            (
                Some(skin.name.as_str()),
                skin.shell
                    .or_else(|| crate::theme_presets::shell_palette(&skin.name))
                    .unwrap_or_default(),
            )
        }))
        .collect();
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
    for row in choices.chunks(columns) {
        ui.horizontal(|ui| {
            for &(name, palette) in row {
                let selected = theme.skin.as_deref() == name;
                let label = name.unwrap_or("系统默认");
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, 76.0), egui::Sense::click());
                let thumb = egui::Rect::from_min_max(
                    rect.min + egui::vec2(2.0, 2.0),
                    rect.right_top() + egui::vec2(-2.0, 52.0),
                );
                thumbnail(ui, thumb, palette.colors(dark));
                let active = crate::theme::shell(ui);
                ui.painter().rect_stroke(
                    thumb,
                    6.0,
                    if selected || response.has_focus() {
                        egui::Stroke::new(2.0, active.accent)
                    } else if response.hovered() {
                        egui::Stroke::new(1.0, active.secondary)
                    } else {
                        separator(ui)
                    },
                    egui::StrokeKind::Outside,
                );
                let label_rect =
                    egui::Rect::from_min_max(rect.left_top() + egui::vec2(0.0, 55.0), rect.max);
                centered_label(
                    ui,
                    label_rect,
                    label,
                    if selected {
                        active.accent
                    } else {
                        active.secondary
                    },
                );
                if selected {
                    ui.painter().circle_filled(
                        thumb.right_top() + egui::vec2(-8.0, 8.0),
                        3.0,
                        active.accent,
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        true,
                        selected,
                        label,
                    )
                });
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("{label} · 应用于整个工作台"))
                    .clicked()
                    && !selected
                {
                    outbox.push(Message::ThemeSkinSelected(name.map(str::to_owned)));
                }
            }
        });
    }
}

fn thumbnail(ui: &egui::Ui, rect: egui::Rect, colors: ShellTokens) {
    let p = ui.painter();
    p.rect_filled(rect, 6.0, colors.chrome);
    let body = egui::Rect::from_min_max(rect.min + egui::vec2(0.0, 12.0), rect.max);
    p.rect_filled(
        body,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: 6,
            se: 6,
        },
        colors.content,
    );
    let side = egui::Rect::from_min_max(
        body.min,
        egui::pos2(body.left() + rect.width() * 0.26, body.bottom()),
    );
    p.rect_filled(
        side,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: 6,
            se: 0,
        },
        colors.sidebar,
    );
    for i in 0..3 {
        p.circle_filled(
            rect.min + egui::vec2(6.0 + i as f32 * 5.0, 6.0),
            1.4,
            colors.secondary,
        );
        p.rect_filled(
            egui::Rect::from_min_size(
                side.min + egui::vec2(4.0, 5.0 + i as f32 * 8.0),
                egui::vec2(side.width() - 8.0, 3.0),
            ),
            1.0,
            if i == 0 {
                colors.selected_bg
            } else {
                colors.border
            },
        );
    }
    let x = side.right() + 7.0;
    let available = rect.right() - x - 7.0;
    for (i, fraction) in [0.55, 0.9, 0.76, 0.62].iter().enumerate() {
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(x, body.top() + 6.0 + i as f32 * 7.0),
                egui::vec2(available * fraction, 2.0),
            ),
            1.0,
            if i == 0 {
                colors.accent
            } else {
                colors.secondary
            },
        );
    }
    p.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(x, body.bottom() - 9.0),
            body.right_bottom() - egui::vec2(7.0, 4.0),
        ),
        2.0,
        colors.code_bg,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_selects_skin_and_restores_default_at_different_widths() {
        let catalog = SkinCatalog {
            skins: crate::theme_presets::builtins()
                .iter()
                .map(|skin| crate::theme::Skin {
                    name: skin.name.to_owned(),
                    style: skin.style.clone(),
                    shell: crate::theme_presets::shell_palette(skin.name),
                })
                .collect(),
        };
        for width in [320.0, 540.0] {
            let ctx = egui::Context::default();
            let mut theme = ThemeSettings::default();
            for target in ["Nord", "系统默认"] {
                theme.apply(&ctx, ThemeMode::Dark);
                let mut positions = Vec::new();
                for _ in 0..2 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ui| choices(ui, &theme, &catalog, &mut Vec::new()),
                    );
                    positions = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == target => {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                }
                let pos = positions[0];
                let mut messages = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: egui::Modifiers::NONE,
                            },
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| choices(ui, &theme, &catalog, &mut messages),
                );
                output.drop_without_applying_deltas();
                let name = (target != "系统默认").then(|| target.to_owned());
                assert_eq!(messages, vec![Message::ThemeSkinSelected(name.clone())]);
                theme.select_skin(name.as_deref(), &catalog);
            }
        }
    }
}
