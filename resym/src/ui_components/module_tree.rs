use std::cell::RefCell;

use eframe::egui::{self, ScrollArea};

use resym_core::frontend::ModuleList;

use crate::{
    module_tree::{ModuleInfo, ModulePath, ModuleTreeNode},
    module_tree_view::{ModuleTreeView, ModuleTreeViewNode},
};

/// UI component in charge of rendering a tree of PDB modules
/// Warning: not thread-safe, use only in single-threaded contexts
pub struct ModuleTreeComponent {
    /// Tree data
    module_tree_view: ModuleTreeView,
    /// Index of the currently selected module
    selected_module: RefCell<usize>,
}

impl ModuleTreeComponent {
    pub fn new() -> Self {
        Self {
            module_tree_view: ModuleTreeView::new(),
            selected_module: usize::MAX.into(),
        }
    }

    /// Update the list of modules that the tree contains
    pub fn set_module_list(&mut self, module_list: ModuleList) {
        // Generate the module tree
        let mut root_tree_node = ModuleTreeNode::default();
        for (module_path, module_index) in module_list.iter() {
            let module_path = ModulePath::from(module_path.as_str());
            // Add module to the tree
            if let Err(err) = root_tree_node.add_module_by_path(
                module_path,
                ModuleInfo {
                    pdb_index: *module_index,
                },
            ) {
                // Log error and continue
                log::warn!("Failed to add module to tree: {}", err);
            }
        }
        // Get a view of the module tree and store it
        self.module_tree_view = ModuleTreeView::from_tree_node(root_tree_node);
    }

    /// Update/render the UI component
    pub fn update<CB: Fn(&ModulePath, &ModuleInfo)>(
        &self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        on_module_selected: &CB,
    ) {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.module_tree_view.children.iter().for_each(|view_node| {
                    self.update_module_tree(ctx, ui, view_node, on_module_selected);
                });
            });
    }

    fn update_module_tree<CB: Fn(&ModulePath, &ModuleInfo)>(
        &self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        view_node: &ModuleTreeViewNode,
        on_module_selected: &CB,
    ) {
        if view_node.is_leaf() {
            self.update_module_leaf(ui, view_node, on_module_selected);
        } else {
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ctx,
                ui.id().with(view_node.path().hash()),
                false,
            )
            .show_header(ui, |ui| {
                ui.label(&view_node.name);
            })
            .body(|ui| {
                view_node.children.iter().for_each(|view_node| {
                    self.update_module_tree(ctx, ui, view_node, on_module_selected);
                });
            });
        }
    }

    fn update_module_leaf<CB: Fn(&ModulePath, &ModuleInfo)>(
        &self,
        ui: &mut egui::Ui,
        view_node: &ModuleTreeViewNode,
        on_module_selected: &CB,
    ) {
        if let Some(ref module_info) = view_node.module_info() {
            if ui
                .selectable_label(
                    *self.selected_module.borrow() == module_info.pdb_index,
                    &view_node.name,
                )
                .clicked()
            {
                *self.selected_module.borrow_mut() = module_info.pdb_index;
                // Invoke event callback
                on_module_selected(view_node.path(), module_info);
            }
        }
    }
}
