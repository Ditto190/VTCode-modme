mod hit_test;
mod layout;
mod render;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use hit_test::visible_index_at_row;

#[expect(
    unused_imports,
    reason = "Intentional compatibility, platform, test, or API-shape suppression."
)]
pub(crate) use layout::{ModalBodyContext, ModalRenderStyles, ModalSection};
#[expect(
    unused_imports,
    reason = "Intentional compatibility, platform, test, or API-shape suppression."
)]
pub(crate) use render::{
    inline_editor_for_step, modal_list_item_lines, render_modal_body, render_modal_list, render_wizard_modal_body,
    render_wizard_tabs,
};
pub use state::{
    ModalKeyModifiers, ModalListItem, ModalListKeyResult, ModalListState, ModalSearchState, ModalState,
    WizardModalState, WizardStepState, is_divider_title,
};
