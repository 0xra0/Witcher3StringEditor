//! Main application UI for Witcher3StringEditor in egui / eframe.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use dictionary_rs::DictionaryManager;
use eframe::egui;
use egui::{Color32, FontData, FontDefinitions, FontFamily, RichText};
use egui_extras::{Column, TableBuilder};
use services_rs::{
    AppPaths, LogAccessService, RecentFilesService, SettingsPersistenceService, ShellOpenService,
};
use settings_rs::port::Settings;
use translate_rs::TranslationClient;
use w3codec_rs::port::{self as codec, W3StringItem};

use super::dialogs::*;

pub struct Witcher3StringEditorApp {
    pub settings: Settings,
    pub items: Vec<W3StringItem>,
    pub file_path: Option<PathBuf>,
    pub output_folder: PathBuf,
    pub search_query: String,
    pub page: usize,
    pub selected_index: Option<usize>,
    pub dark_mode: bool,

    // Services
    pub log_service: LogAccessService,
    pub dict_manager: DictionaryManager,
    pub translation_client: TranslationClient,
    pub update_available: Option<String>,

    // Dialog states
    pub show_save: bool,
    pub save_state: SaveDialogState,
    pub show_settings: bool,
    pub show_backup: bool,
    pub backup_state: BackupDialogState,
    pub show_recent: bool,
    pub recent_state: RecentDialogState,
    pub show_dict: bool,
    pub dict_state: DictDialogState,
    pub show_log: bool,
    pub log_state: LogDialogState,
    pub show_translate: bool,
    pub translate_state: TranslateDialogState,
    pub edit_dialog_state: Option<EditDialogState>,
    pub show_about: bool,

    // Async channels
    pub update_rx: Receiver<Option<String>>,
}

impl Witcher3StringEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);

        let settings = SettingsPersistenceService::load();
        let log_service = LogAccessService::default();
        let dict_manager = DictionaryManager::new(AppPaths::dictionary_directory());
        let translation_client = TranslationClient::default();

        let initial_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let save_state = SaveDialogState::new(&settings, &initial_dir);

        let (update_tx, update_rx) = channel();

        // Check for updates in background
        tokio::spawn(async move {
            let current_v = env!("CARGO_PKG_VERSION");
            if let Ok(newer) = nexus_rs::check_update(current_v).await {
                let _ = update_tx.send(newer);
            }
        });

        log_service.add("INFO", "Witcher3StringEditor initialized");

        Self {
            settings,
            items: Vec::new(),
            file_path: None,
            output_folder: initial_dir,
            search_query: String::new(),
            page: 0,
            selected_index: None,
            dark_mode: true,

            log_service,
            dict_manager,
            translation_client,
            update_available: None,

            show_save: false,
            save_state,
            show_settings: false,
            show_backup: false,
            backup_state: BackupDialogState::default(),
            show_recent: false,
            recent_state: RecentDialogState::default(),
            show_dict: false,
            dict_state: DictDialogState::default(),
            show_log: false,
            log_state: LogDialogState::default(),
            show_translate: false,
            translate_state: TranslateDialogState::default(),
            edit_dialog_state: None,
            show_about: false,

            update_rx,
        }
    }

    pub fn load_file(&mut self, path: &Path) {
        let path_buf = path.to_path_buf();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let res: Result<Vec<W3StringItem>, String> = match ext.as_str() {
            "w3strings" => {
                match fs::read(path) {
                    Ok(bytes) => codec::decode(&bytes).map(|d| d.items),
                    Err(e) => Err(e.to_string()),
                }
            }
            "csv" => {
                match fs::read_to_string(path) {
                    Ok(text) => Ok(codec::csv_decode(&text)),
                    Err(e) => Err(e.to_string()),
                }
            }
            "xlsx" => xlsx_rs::read_xlsx(path),
            _ => Err("Unsupported file format (must be .w3strings, .csv, or .xlsx)".into()),
        };

        match res {
            Ok(mut loaded) => {
                loaded.sort_by(|a, b| a.str_id.cmp(&b.str_id));
                self.items = loaded;
                self.file_path = Some(path_buf.clone());
                if let Some(parent) = path.parent() {
                    self.output_folder = parent.to_path_buf();
                    self.save_state.output_dir = self.output_folder.clone();
                }
                RecentFilesService::add_or_update(&path_buf.to_string_lossy(), &mut self.settings);
                let _ = SettingsPersistenceService::save(&self.settings);
                self.page = 0;
                self.selected_index = None;
                self.log_service.add(
                    "INFO",
                    &format!("Loaded {} items from {}", self.items.len(), path.display()),
                );
            }
            Err(e) => {
                self.log_service.add(
                    "ERROR",
                    &format!("Failed to open file {}: {e}", path.display()),
                );
            }
        }
    }

    pub fn merge_file(&mut self, path: &Path) {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let res = match ext.as_str() {
            "w3strings" => {
                match fs::read(path) {
                    Ok(bytes) => codec::decode(&bytes).map(|d| d.items),
                    Err(e) => Err(e.to_string()),
                }
            }
            "csv" => {
                match fs::read_to_string(path) {
                    Ok(text) => Ok(codec::csv_decode(&text)),
                    Err(e) => Err(e.to_string()),
                }
            }
            "xlsx" => xlsx_rs::read_xlsx(path),
            _ => Err("Unsupported file format".into()),
        };

        if let Ok(new_items) = res {
            let mut matched = 0;
            for new_item in new_items {
                if let Some(existing) = self.items.iter_mut().find(|it| it.str_id == new_item.str_id)
                    && existing.text != new_item.text
                {
                    if existing.old_text.is_empty() {
                        existing.old_text = existing.text.clone();
                    }
                    existing.text = new_item.text;
                    matched += 1;
                }
            }
            self.log_service.add(
                "INFO",
                &format!("Merged {matched} item texts from {}", path.display()),
            );
        }
    }

    pub fn create_next_str_id(&self) -> String {
        let mut highest: u32 = 0;
        let mut found = false;
        for item in &self.items {
            if let Ok(id) = item.str_id.parse::<u32>()
                && (!found || id > highest)
            {
                highest = id;
                found = true;
            }
        }
        if found && highest < u32::MAX {
            (highest + 1).to_string()
        } else {
            String::new()
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search_query.trim().to_ascii_lowercase();
        if q.is_empty() {
            (0..self.items.len()).collect()
        } else {
            self.items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.str_id.to_ascii_lowercase().contains(&q)
                        || item.key_name.to_ascii_lowercase().contains(&q)
                        || item.key_hex.to_ascii_lowercase().contains(&q)
                        || item.text.to_ascii_lowercase().contains(&q)
                })
                .map(|(idx, _)| idx)
                .collect()
        }
    }
}

impl eframe::App for Witcher3StringEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme
        if self.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Check async update notification
        if let Ok(newer) = self.update_rx.try_recv() {
            self.update_available = newer;
        }

        // Drag & drop file handler
        ctx.input(|i| {
            if let Some(file) = i.raw.dropped_files.first()
                && let Some(path) = &file.path
            {
                self.load_file(path);
            }
        });

        // Top Header and Toolbar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("The Witcher 3 String Editor").strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme_icon = if self.dark_mode { "☀ Light" } else { "🌙 Dark" };
                    if ui.button(theme_icon).clicked() {
                        self.dark_mode = !self.dark_mode;
                    }

                    // Search box
                    ui.label("Search:");
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search ID, Key or Text...")
                            .desired_width(220.0),
                    );
                    if search_resp.changed() {
                        self.page = 0;
                    }
                    if !self.search_query.is_empty() && ui.small_button("✖").clicked() {
                        self.search_query.clear();
                        self.page = 0;
                    }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Command Bar / Toolbar
            ui.horizontal_wrapped(|ui| {
                if ui.button(RichText::new("📂 Open").strong()).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Witcher 3 Strings", &["w3strings", "csv", "xlsx"])
                        .pick_file()
                {
                    self.load_file(&path);
                }

                if ui.button("🕒 Recent").clicked() {
                    self.show_recent = true;
                }

                let has_items = !self.items.is_empty();

                ui.add_enabled_ui(has_items, |ui| {
                    if ui.button("🔀 Merge").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Witcher 3 Strings", &["w3strings", "csv", "xlsx"])
                            .pick_file()
                    {
                        self.merge_file(&path);
                    }
                });

                ui.separator();

                if ui.button("➕ Add").clicked() {
                    let next_id = self.create_next_str_id();
                    self.edit_dialog_state = Some(EditDialogState::for_add(next_id));
                }

                let has_selection = self.selected_index.is_some();
                ui.add_enabled_ui(has_selection, |ui| {
                    if ui.button("✏ Edit").clicked()
                        && let Some(sel) = self.selected_index
                        && sel < self.items.len()
                    {
                        self.edit_dialog_state = Some(EditDialogState::for_edit(sel, &self.items[sel]));
                    }

                    if ui.button("🗑 Delete").clicked()
                        && let Some(sel) = self.selected_index
                        && sel < self.items.len()
                    {
                        self.items.remove(sel);
                        self.selected_index = None;
                    }
                });

                ui.separator();

                ui.add_enabled_ui(has_items, |ui| {
                    if ui.button("🌐 Translate").clicked() {
                        self.show_translate = true;
                    }
                });

                if ui.button("📖 Dictionary").clicked() {
                    self.dict_manager.reload();
                    self.show_dict = true;
                }

                ui.separator();

                ui.add_enabled_ui(has_items, |ui| {
                    if ui.button(RichText::new("💾 Save").strong()).clicked() {
                        self.save_state = SaveDialogState::new(&self.settings, &self.output_folder);
                        self.show_save = true;
                    }
                });

                if ui.button("🛡 Backup").clicked() {
                    self.show_backup = true;
                }

                if ui.button("📋 Log").clicked() {
                    self.show_log = true;
                }

                if ui.button("⚙ Settings").clicked() {
                    self.show_settings = true;
                }

                if ui.button("📁 Folder").clicked() {
                    let _ = ShellOpenService::open(&self.output_folder.to_string_lossy());
                }

                if ui.button("🔗 Nexus").clicked() {
                    let _ = ShellOpenService::open(nexus_rs::NEXUS_MOD_URL);
                }

                if ui.button("ℹ About").clicked() {
                    self.show_about = true;
                }
            });

            ui.add_space(4.0);
        });

        // Bottom Status and Paging Panel
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(4.0);

            // Update banner
            if let Some(new_ver) = &self.update_available {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!("🔔 New update available: v{new_ver}! Click Nexus in toolbar to download."),
                    );
                });
                ui.separator();
            }

            ui.horizontal(|ui| {
                let filtered = self.filtered_indices();
                let total = self.items.len();
                let filtered_count = filtered.len();

                if self.search_query.trim().is_empty() {
                    ui.label(format!("Total: {total} items"));
                } else {
                    ui.label(format!("Showing {filtered_count} of {total} items"));
                }

                if let Some(path) = &self.file_path {
                    ui.label(format!("| Current file: {}", path.display()));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let page_size = self.settings.page_size.max(1) as usize;
                    let total_pages = filtered_count.div_ceil(page_size);
                    let cur_page = self.page.min(total_pages.saturating_sub(1));

                    if ui.button("Next >").clicked() && cur_page + 1 < total_pages {
                        self.page = cur_page + 1;
                    }

                    ui.label(format!("Page {} of {}", cur_page + 1, total_pages.max(1)));

                    if ui.button("< Prev").clicked() && cur_page > 0 {
                        self.page = cur_page - 1;
                    }
                });
            });
            ui.add_space(4.0);
        });

        // Center Panel: Data Grid or Empty Drop Zone
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.items.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.label(RichText::new("Drag and drop a .w3strings, .csv, or .xlsx file here").size(20.0).color(Color32::GRAY));
                        ui.add_space(12.0);
                        ui.label(RichText::new("Or click Open in the toolbar to browse for files").size(14.0).color(Color32::DARK_GRAY));
                    });
                });
            } else {
                let filtered = self.filtered_indices();
                let page_size = self.settings.page_size.max(1) as usize;
                let total_pages = filtered.len().div_ceil(page_size);
                if self.page >= total_pages && total_pages > 0 {
                    self.page = total_pages - 1;
                }

                let start = self.page * page_size;
                let end = (start + page_size).min(filtered.len());
                let page_slice = if start < filtered.len() {
                    &filtered[start..end]
                } else {
                    &[]
                };

                let mut to_reset: Option<usize> = None;
                let mut row_clicked: Option<usize> = None;

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(90.0))   // StrId
                    .column(Column::exact(100.0))  // KeyHex
                    .column(Column::exact(220.0))  // KeyName
                    .column(Column::remainder())   // Text
                    .column(Column::exact(70.0))   // Reset
                    .header(24.0, |mut header| {
                        header.col(|ui| { ui.strong("ID"); });
                        header.col(|ui| { ui.strong("Key(Hex)"); });
                        header.col(|ui| { ui.strong("Key(String)"); });
                        header.col(|ui| { ui.strong("Text"); });
                        header.col(|ui| { ui.strong("Reset"); });
                    })
                    .body(|mut body| {
                        for &real_idx in page_slice {
                            let item = &self.items[real_idx];
                            let is_selected = self.selected_index == Some(real_idx);

                            body.row(28.0, |mut row| {
                                if is_selected {
                                    row.set_selected(true);
                                }

                                row.col(|ui| {
                                    let resp = ui.selectable_label(is_selected, &item.str_id);
                                    if resp.clicked() {
                                        row_clicked = Some(real_idx);
                                    }
                                });

                                row.col(|ui| {
                                    ui.label(RichText::new(&item.key_hex).monospace());
                                });

                                row.col(|ui| {
                                    ui.label(&item.key_name);
                                });

                                row.col(|ui| {
                                    let is_mod = !item.old_text.is_empty() && item.old_text != item.text;
                                    let text_color = if is_mod {
                                        Color32::from_rgb(100, 200, 255)
                                    } else {
                                        ui.visuals().text_color()
                                    };
                                    ui.label(RichText::new(&item.text).color(text_color));
                                });

                                row.col(|ui| {
                                    let has_mod = !item.old_text.is_empty() && item.old_text != item.text;
                                    ui.add_enabled_ui(has_mod, |ui| {
                                        if ui.small_button("Undo").clicked() {
                                            to_reset = Some(real_idx);
                                        }
                                    });
                                });
                            });
                        }
                    });

                if let Some(idx) = row_clicked {
                    self.selected_index = Some(idx);
                }

                if let Some(idx) = to_reset
                    && idx < self.items.len() && !self.items[idx].old_text.is_empty()
                {
                    let original = self.items[idx].old_text.clone();
                    self.items[idx].text = original;
                    self.items[idx].old_text.clear();
                }
            }
        });

        // Dialog Windows
        if self.show_save {
            show_save_dialog(
                ctx,
                &mut self.save_state,
                &self.items,
                &mut self.settings,
                &mut self.show_save,
            );
        }

        if self.show_settings {
            show_settings_dialog(
                ctx,
                &mut self.settings,
                &mut self.dark_mode,
                &mut self.show_settings,
            );
        }

        if self.show_backup {
            show_backup_dialog(
                ctx,
                &mut self.backup_state,
                &mut self.settings,
                &mut self.show_backup,
            );
        }

        if self.show_recent {
            show_recent_dialog(
                ctx,
                &mut self.recent_state,
                &mut self.settings,
                &mut self.show_recent,
            );
            if let Some(path_str) = self.recent_state.open_file.take() {
                self.load_file(Path::new(&path_str));
            }
        }

        if self.show_dict {
            show_dict_dialog(
                ctx,
                &mut self.dict_state,
                &mut self.dict_manager,
                &self.settings,
                &mut self.show_dict,
            );
        }

        if self.show_log {
            show_log_dialog(
                ctx,
                &mut self.log_state,
                &self.log_service,
                &self.settings,
                &mut self.show_log,
            );
        }

        if self.show_translate {
            show_translate_dialog(
                ctx,
                &mut self.translate_state,
                &mut self.items,
                &self.translation_client,
                &self.settings,
                &mut self.show_translate,
            );
        }

        if self.edit_dialog_state.is_some() {
            show_edit_dialog(
                ctx,
                &mut self.edit_dialog_state,
                &mut self.items,
                &self.settings,
            );
        }

        if self.show_about {
            show_about_dialog(ctx, &self.settings, &mut self.show_about);
        }
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Check system fonts for multi-script rendering
    let fallback_paths = [
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/droid-nonlatin/DroidSansThai.ttf",
        "/usr/share/fonts/droid-nonlatin/DroidSansHebrew-Regular.ttf",
        "/usr/share/fonts/droid-nonlatin/DroidSansArabic.ttf",
    ];

    for path in fallback_paths {
        if let Ok(data) = fs::read(path) {
            let font_name = Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "fallback".into());
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(data).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push(font_name.clone());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push(font_name);
        }
    }

    ctx.set_fonts(fonts);
}
