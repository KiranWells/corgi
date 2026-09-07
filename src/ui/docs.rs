use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use eframe::egui::{self, Button};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_material_icons::icons;
use parking_lot::Mutex;

use crate::ui::resources_directory;

#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum DocsPage {
    #[default]
    Home,
    QuickStart,
    Styling,
    DataExport,
    Keybinds,
}

impl DocsPage {
    const ALL: [DocsPage; 5] = [
        DocsPage::Home,
        DocsPage::QuickStart,
        DocsPage::Styling,
        DocsPage::DataExport,
        DocsPage::Keybinds,
    ];
    fn link(self) -> &'static str {
        match self {
            DocsPage::Home => "docs/README.md",
            DocsPage::QuickStart => "docs/quick_start.md",
            DocsPage::Styling => "docs/styling.md",
            DocsPage::DataExport => "docs/data_export.md",
            DocsPage::Keybinds => "docs/keybinds.md",
        }
    }
}

#[derive(Debug)]
pub(super) struct DocsState {
    pub active: bool,
    page: DocsPage,
    history: Vec<DocsPage>,
    future: Vec<DocsPage>,
    cache: CommonMarkCache,
}

impl Default for DocsState {
    fn default() -> Self {
        let mut cache = CommonMarkCache::default();
        for page in DocsPage::ALL {
            cache.add_link_hook(format!("/{}", page.link()));
        }
        Self {
            active: false,
            page: DocsPage::Home,
            history: vec![],
            future: vec![],
            cache,
        }
    }
}

static DOCS_CACHE: LazyLock<Mutex<HashMap<DocsPage, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn prepare_docs(page: DocsPage) -> String {
    DOCS_CACHE
        .lock()
        .entry(page)
        .or_insert_with(|| match prepare_docs_inner(page.link()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!("Failed to load docs: {err}");
                format!(
                    r"# Errorhash

Failed to load docs:

```
{err}
```
"
                )
            }
        })
        .clone()
}

fn prepare_docs_inner(path: impl AsRef<Path>) -> color_eyre::Result<String> {
    let doc = std::fs::read_to_string(resources_directory().join(path))?;

    Ok(doc.replace(
        "(./",
        &format!("({}/docs/", resources_directory().to_string_lossy()),
    ))
}

impl DocsState {
    pub(super) fn goto(&mut self, page: DocsPage) {
        self.history.push(self.page);
        self.future = vec![];
        self.page = page;
    }

    pub(super) fn render_docs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(icons::ICON_HOME).clicked() {
                self.goto(DocsPage::Home)
            }
            if ui
                .add_enabled(
                    !self.history.is_empty(),
                    Button::new(icons::ICON_ARROW_BACK),
                )
                .clicked()
            {
                self.future.push(self.page);
                self.page = self.history.pop().unwrap();
            }
            if ui
                .add_enabled(
                    !self.future.is_empty(),
                    Button::new(icons::ICON_ARROW_FORWARD),
                )
                .clicked()
            {
                self.history.push(self.page);
                self.page = self.future.pop().unwrap();
            }
        });

        let viewer = CommonMarkViewer::new().enable_scroll_to_heading(true);

        egui::ScrollArea::vertical()
            .id_salt(self.page)
            .show(ui, |ui| {
                egui::Frame::new()
                    .outer_margin(ui.spacing().indent)
                    .show(ui, |ui| {
                        viewer.show(ui, &mut self.cache, &prepare_docs(self.page));
                        for page in DocsPage::ALL {
                            if self
                                .cache
                                .get_link_hook(&format!("/{}", page.link()))
                                .unwrap()
                            {
                                self.goto(page)
                            }
                        }
                    });
            });
    }
}
