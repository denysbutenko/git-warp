mod apply;
mod model;
mod spec;
mod tui;
mod view;

#[cfg(test)]
mod tests;

pub use apply::{apply_field_value, detect_env_overrides, render_field_value};
pub use model::{ConfigEditorModel, ConfigQuitOutcome, ConfigStatusKind, ConfigStatusMsg};
pub use spec::{
    ConfigFieldId, ConfigFieldKind, ConfigFieldSpec, ConfigSectionId, config_field_specs,
};
pub use tui::ConfigTui;
pub use view::draw_config_editor;
