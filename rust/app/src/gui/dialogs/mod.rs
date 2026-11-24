//! Dialog windows for the Witcher3StringEditor egui application.
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Window};
use locales_rs as locales;
use services_rs::{AppPaths, BackupItem, BackupService, LogAccessService, RecentFilesService, ShellOpenService};
use settings_rs::port::{FileType, PayloadEncoding, Settings};
use translate_rs::TranslationClient;
use w3codec_rs::port::{self as codec, W3StringItem};

// ---------------------------------------------------------------------------
// Save Dialog
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SaveDialogState {
    pub format: FileType,
    pub language_code: String,
    pub encoding: PayloadEncoding,
    pub output_dir: PathBuf,
    pub message: Option<(bool, String)>,
}

impl SaveDialogState {
    pub fn new(settings: &Settings, initial_dir: &Path) -> Self {
        Self {
            format: settings.preferred_file_type.clone(),
            language_code: settings.preferred_language.clone(),
            encoding: settings.preferred_encoding,
            output_dir: initial_dir.to_path_buf(),
            message: None,
        }
    }
}

pub fn show_save_dialog(
    ctx: &egui::Context,
    state: &mut SaveDialogState,
    items: &[W3StringItem],
    settings: &mut Settings,
    open: &mut bool,
) -> bool {
    let mut saved = false;
    let mut is_open = *open;
    let mut close = false;
    Window::new(locales::get(&settings.language, "SaveDialogTitle"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .default_width(450.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // Format Selection
            ui.horizontal(|ui| {
                ui.label(RichText::new("Target Format:").strong());
                ui.radio_value(&mut state.format, FileType::W3Strings, "W3Strings");
                ui.radio_value(&mut state.format, FileType::Csv, "CSV");
                ui.radio_value(&mut state.format, FileType::Excel, "Excel (XLSX)");
            });

            ui.add_space(8.0);

            // Language Selection
            ui.horizontal(|ui| {
                ui.label(RichText::new("Target Language:").strong());
                let langs = codec::ALL_LANGUAGES;
                let current_name = langs
                    .iter()
                    .find(|l| l.code.eq_ignore_ascii_case(&state.language_code))
                    .map(|l| format!("{} ({})", l.culture, l.code.to_uppercase()))
                    .unwrap_or_else(|| state.language_code.to_uppercase());

                egui::ComboBox::from_id_salt("save_lang_combo")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for lang in langs {
                            let text = format!("{} ({})", lang.culture, lang.code.to_uppercase());
                            ui.selectable_value(
                                &mut state.language_code,
                                lang.code.to_string(),
                                text,
                            );
                        }
                    });
            });

            // Encoding (only for W3Strings)
            if state.format == FileType::W3Strings {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Encoding:").strong());
                    ui.radio_value(
                        &mut state.encoding,
                        PayloadEncoding::Utf16Le,
                        "Unicode (UTF-16 LE - Classic)",
                    );
                    ui.radio_value(
                        &mut state.encoding,
                        PayloadEncoding::Utf8,
                        "UTF-8 (Next-Gen)",
                    );
                });
            }

            ui.add_space(8.0);

            // Output Directory
            ui.horizontal(|ui| {
                ui.label(RichText::new("Directory:").strong());
                let mut path_str = state.output_dir.to_string_lossy().to_string();
                if ui.text_edit_singleline(&mut path_str).changed() {
                    state.output_dir = PathBuf::from(path_str);
                }
                if ui.button("Browse...").clicked()
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_directory(&state.output_dir)
                        .pick_folder()
                {
                    state.output_dir = folder;
                }
            });

            if let Some((success, msg)) = &state.message {
                ui.add_space(8.0);
                let color = if *success { Color32::GREEN } else { Color32::RED };
                ui.colored_label(color, msg);
            }

            ui.add_space(16.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button(RichText::new("Save").strong()).clicked() {
                    let lang = codec::language_from_code(&state.language_code)
                        .unwrap_or(codec::W3Language {
                            key: 0,
                            code: "en",
                            culture: "en-US",
                            magic: 0x57335354,
                        });

                    let ext = match state.format {
                        FileType::W3Strings => "w3strings",
                        FileType::Csv => "csv",
                        FileType::Excel => "xlsx",
                    };
                    let file_path = state
                        .output_dir
                        .join(format!("{}.{}", lang.code.to_ascii_lowercase(), ext));

                    let res = save_items(&file_path, items, state, lang, || {
                        if file_path.try_exists().map_err(|e| e.to_string())? {
                            BackupService::backup(&file_path, settings)
                                .map_err(|e| format!("Could not back up the existing file: {e}"))?;
                            services_rs::SettingsPersistenceService::save(settings)
                                .map_err(|e| format!("Could not save the backup record: {e}"))?;
                        }
                        Ok(())
                    });

                    match res {
                        Ok(()) => {
                            saved = true;
                            state.message = Some((
                                true,
                                format!("Successfully saved {} items to {}", items.len(), file_path.display()),
                            ));
                            settings.preferred_language = state.language_code.clone();
                            settings.preferred_file_type = state.format.clone();
                            settings.preferred_encoding = state.encoding;
                            let _ = services_rs::SettingsPersistenceService::save(settings);
                        }
                        Err(e) => {
                            state.message = Some((false, format!("Failed to save: {e}")));
                        }
                    }
                }

                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    if close {
        is_open = false;
    }
    *open = is_open;
    saved
}

fn save_items(
    path: &Path,
    items: &[W3StringItem],
    state: &SaveDialogState,
    lang: codec::W3Language,
    prepare_overwrite: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let bytes = match state.format {
        FileType::W3Strings => {
            let version = match state.encoding {
                PayloadEncoding::Utf8 => 164,
                PayloadEncoding::Utf16Le => 162,
            };
            codec::encode(items, lang.key, version).ok_or("Failed to encode w3strings file")?
        }
        FileType::Csv => {
            if let Some(item) = items.iter().find(|item| {
                [&item.str_id, &item.key_hex, &item.key_name, &item.text]
                    .into_iter()
                    .any(|field| field.contains(['\r', '\n']))
            }) {
                return Err(format!(
                    "CSV cannot preserve line breaks (string ID {}). Choose W3Strings or Excel.",
                    item.str_id
                ));
            }
            codec::csv_encode(items, lang.code, lang.magic).into_bytes()
        }
        FileType::Excel => {
            prepare_overwrite()?;
            return xlsx_rs::write_xlsx(path, items);
        }
    };
    prepare_overwrite()?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Settings Dialog
// ---------------------------------------------------------------------------

pub fn show_settings_dialog(
    ctx: &egui::Context,
    settings: &mut Settings,
    dark_mode: &mut bool,
    open: &mut bool,
) {
    let mut is_open = *open;
    let mut close = false;
    Window::new(locales::get(&settings.language, "SettingsDialogTitle"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .default_width(450.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // Preferred File Type
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preferred File Type:").strong());
                ui.radio_value(&mut settings.preferred_file_type, FileType::Csv, "CSV");
                ui.radio_value(
                    &mut settings.preferred_file_type,
                    FileType::W3Strings,
                    "W3Strings",
                );
                ui.radio_value(
                    &mut settings.preferred_file_type,
                    FileType::Excel,
                    "Excel",
                );
            });

            ui.add_space(6.0);

            // Preferred Encoding
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preferred Encoding:").strong());
                ui.radio_value(
                    &mut settings.preferred_encoding,
                    PayloadEncoding::Utf16Le,
                    "Unicode (UTF-16 LE)",
                );
                ui.radio_value(
                    &mut settings.preferred_encoding,
                    PayloadEncoding::Utf8,
                    "UTF-8",
                );
            });

            ui.add_space(6.0);

            // Preferred Game Language
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preferred Language:").strong());
                let langs = codec::ALL_LANGUAGES;
                let cur = langs
                    .iter()
                    .find(|l| l.code.eq_ignore_ascii_case(&settings.preferred_language))
                    .map(|l| format!("{} ({})", l.culture, l.code.to_uppercase()))
                    .unwrap_or_else(|| settings.preferred_language.to_uppercase());

                egui::ComboBox::from_id_salt("pref_lang_combo")
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        for lang in langs {
                            let text = format!("{} ({})", lang.culture, lang.code.to_uppercase());
                            ui.selectable_value(
                                &mut settings.preferred_language,
                                lang.code.to_string(),
                                text,
                            );
                        }
                    });
            });

            ui.add_space(6.0);

            // UI Locale
            ui.horizontal(|ui| {
                ui.label(RichText::new("UI Language:").strong());
                let cur_locale = locales::SUPPORTED_CULTURES
                    .iter()
                    .find(|(code, _)| code.eq_ignore_ascii_case(&settings.language))
                    .map(|(_, name)| *name)
                    .unwrap_or("English");

                egui::ComboBox::from_id_salt("ui_locale_combo")
                    .selected_text(cur_locale)
                    .show_ui(ui, |ui| {
                        for &(code, name) in locales::SUPPORTED_CULTURES {
                            ui.selectable_value(&mut settings.language, code.to_string(), name);
                        }
                    });
            });

            ui.add_space(6.0);

            // Translator
            ui.horizontal(|ui| {
                ui.label(RichText::new("Translator Provider:").strong());
                egui::ComboBox::from_id_salt("translator_combo")
                    .selected_text(&settings.translator)
                    .show_ui(ui, |ui| {
                        for &name in translate_rs::TRANSLATOR_NAMES {
                            ui.selectable_value(&mut settings.translator, name.to_string(), name);
                        }
                    });
            });

            ui.add_space(6.0);

            // Page Size
            ui.horizontal(|ui| {
                ui.label(RichText::new("Page Size:").strong());
                ui.radio_value(&mut settings.page_size, 20, "20");
                ui.radio_value(&mut settings.page_size, 30, "30");
                ui.radio_value(&mut settings.page_size, 50, "50");
                ui.radio_value(&mut settings.page_size, 100, "100");
            });

            ui.add_space(6.0);

            // Theme toggle
            ui.horizontal(|ui| {
                ui.label(RichText::new("Theme:").strong());
                ui.radio_value(dark_mode, true, "Dark");
                ui.radio_value(dark_mode, false, "Light");
            });

            ui.add_space(16.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Save & Close").clicked() {
                    let _ = services_rs::SettingsPersistenceService::save(settings);
                    close = true;
                }
            });
        });
    if close {
        is_open = false;
    }
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Backup Dialog
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct BackupDialogState {
    pub search: String,
    pub page: usize,
    pub message: Option<(bool, String)>,
}

pub fn show_backup_dialog(
    ctx: &egui::Context,
    state: &mut BackupDialogState,
    settings: &mut Settings,
    open: &mut bool,
) {
    let mut is_open = *open;
    Window::new(locales::get(&settings.language, "BackupDialogTitle"))
        .open(&mut is_open)
        .default_width(700.0)
        .default_height(450.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut state.search);
            });

            if let Some((success, msg)) = &state.message {
                ui.add_space(4.0);
                let color = if *success { Color32::GREEN } else { Color32::RED };
                ui.colored_label(color, msg);
            }

            ui.add_space(8.0);

            let search_lower = state.search.to_ascii_lowercase();
            let filtered: Vec<BackupItem> = settings
                .backup_items
                .iter()
                .filter(|b| {
                    if search_lower.is_empty() {
                        true
                    } else {
                        b.file_name.to_ascii_lowercase().contains(&search_lower)
                            || b.hash.to_ascii_lowercase().contains(&search_lower)
                            || b.orgin_path.to_ascii_lowercase().contains(&search_lower)
                    }
                })
                .cloned()
                .collect();

            let page_size = 10;
            let total_pages = filtered.len().div_ceil(page_size);
            let current_page = state.page.min(total_pages.saturating_sub(1));
            let start = current_page * page_size;
            let end = (start + page_size).min(filtered.len());
            let page_items = if start < filtered.len() {
                &filtered[start..end]
            } else {
                &[]
            };

            let mut to_restore: Option<BackupItem> = None;
            let mut to_delete: Option<BackupItem> = None;

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("backup_grid")
                    .striped(true)
                    .num_columns(5)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("File Name").strong());
                        ui.label(RichText::new("Original Path").strong());
                        ui.label(RichText::new("Backup Date").strong());
                        ui.label(RichText::new("Hash").strong());
                        ui.label(RichText::new("Actions").strong());
                        ui.end_row();

                        for item in page_items {
                            ui.label(&item.file_name);
                            ui.label(&item.orgin_path);
                            ui.label(&item.backup_time.date_time);
                            let short_hash = if item.hash.len() > 8 {
                                &item.hash[..8]
                            } else {
                                &item.hash
                            };
                            ui.label(short_hash);

                            ui.horizontal(|ui| {
                                if ui.button("Restore").clicked() {
                                    to_restore = Some(item.clone());
                                }
                                if ui.button("Delete").clicked() {
                                    to_delete = Some(item.clone());
                                }
                            });
                            ui.end_row();
                        }
                    });
            });

            if let Some(item) = to_restore {
                match BackupService::restore(&item) {
                    Ok(()) => {
                        state.message = Some((true, format!("Restored backup for {}", item.file_name)));
                    }
                    Err(e) => {
                        state.message = Some((false, format!("Restore failed: {e}")));
                    }
                }
            }

            if let Some(item) = to_delete {
                let _ = BackupService::delete(&item, settings);
                let _ = services_rs::SettingsPersistenceService::save(settings);
                state.message = Some((true, format!("Deleted backup for {}", item.file_name)));
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("< Prev").clicked() && state.page > 0 {
                    state.page -= 1;
                }
                ui.label(format!("Page {} of {}", current_page + 1, total_pages.max(1)));
                if ui.button("Next >").clicked() && current_page + 1 < total_pages {
                    state.page += 1;
                }
            });
        });
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Recent Files Dialog
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RecentDialogState {
    pub search: String,
    pub open_file: Option<String>,
}

pub fn show_recent_dialog(
    ctx: &egui::Context,
    state: &mut RecentDialogState,
    settings: &mut Settings,
    open: &mut bool,
) {
    let mut is_open = *open;
    let mut close = false;
    Window::new(locales::get(&settings.language, "RecentDialogTitle"))
        .open(&mut is_open)
        .default_width(650.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut state.search);
            });

            ui.add_space(8.0);

            let search_lower = state.search.to_ascii_lowercase();
            let mut to_remove: Option<String> = None;
            let mut to_toggle: Option<String> = None;

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("recent_grid")
                    .striped(true)
                    .num_columns(4)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Pinned").strong());
                        ui.label(RichText::new("File Path").strong());
                        ui.label(RichText::new("Opened Time").strong());
                        ui.label(RichText::new("Actions").strong());
                        ui.end_row();

                        for item in &settings.recent_items {
                            if !search_lower.is_empty()
                                && !item.file_path.to_ascii_lowercase().contains(&search_lower)
                            {
                                continue;
                            }

                            let mut marked = item.is_marked;
                            if ui.checkbox(&mut marked, "").changed() {
                                to_toggle = Some(item.file_path.clone());
                            }

                            ui.label(&item.file_path);
                            ui.label(&item.opened_time.date_time);

                            ui.horizontal(|ui| {
                                if ui.button("Open").clicked() {
                                    state.open_file = Some(item.file_path.clone());
                                    close = true;
                                }
                                if ui.button("Delete").clicked() {
                                    to_remove = Some(item.file_path.clone());
                                }
                            });
                            ui.end_row();
                        }
                    });
            });

            if let Some(path) = to_toggle {
                RecentFilesService::toggle_mark(&path, settings);
                let _ = services_rs::SettingsPersistenceService::save(settings);
            }

            if let Some(path) = to_remove {
                RecentFilesService::remove(&path, settings);
                let _ = services_rs::SettingsPersistenceService::save(settings);
            }
        });
    if close {
        is_open = false;
    }
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Dictionary Manager Dialog
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct DictDialogState {
    pub selected_dict: Option<usize>,
    pub terms: Vec<(String, String)>,
    pub search: String,
    pub message: Option<(bool, String)>,
}

pub fn show_dict_dialog(
    ctx: &egui::Context,
    state: &mut DictDialogState,
    manager: &mut dictionary_rs::DictionaryManager,
    settings: &Settings,
    open: &mut bool,
) {
    let mut is_open = *open;
    Window::new(locales::get(&settings.language, "DictionaryManagerDialogTitle"))
        .open(&mut is_open)
        .default_width(750.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Import Dictionary (.txt)...").clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .add_filter("Dictionary Text", &["txt"])
                        .pick_file()
                {
                    match manager.import(&file) {
                        Ok(info) => {
                            state.message = Some((true, format!("Imported {}", info.name)));
                        }
                        Err(e) => {
                            state.message = Some((false, format!("Import failed: {e}")));
                        }
                    }
                }

                if let Some(idx) = state.selected_dict
                    && let Some(dict) = manager.dictionaries().get(idx)
                {
                    let path = dict.path.clone();
                    if ui.button("Delete Selected Dictionary").clicked() {
                        manager.remove_by_path(&path);
                        state.selected_dict = None;
                        state.terms.clear();
                    }
                }
            });

            if let Some((success, msg)) = &state.message {
                ui.add_space(4.0);
                let color = if *success { Color32::GREEN } else { Color32::RED };
                ui.colored_label(color, msg);
            }

            ui.add_space(8.0);
            ui.separator();

            ui.columns(2, |columns| {
                // Column 0: Dictionary list
                columns[0].label(RichText::new("Installed Dictionaries:").strong());
                egui::ScrollArea::vertical().max_height(350.0).show(&mut columns[0], |ui| {
                    let dicts = manager.dictionaries().to_vec();
                    for (i, d) in dicts.iter().enumerate() {
                        let is_sel = state.selected_dict == Some(i);
                        let label = format!("{} ({} -> {}, {} terms)", d.name, d.source_language, d.target_language, d.term_count);
                        if ui.selectable_label(is_sel, label).clicked() {
                            state.selected_dict = Some(i);
                            if let Ok(content) = std::fs::read_to_string(&d.path) {
                                state.terms = dictionary_rs::parse_entries(&content);
                            }
                        }
                    }
                });

                // Column 1: Terms
                columns[1].label(RichText::new("Terms in Dictionary:").strong());
                columns[1].horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut state.search);
                });
                let filter = state.search.to_ascii_lowercase();

                egui::ScrollArea::vertical().max_height(350.0).show(&mut columns[1], |ui| {
                    egui::Grid::new("dict_terms_grid").striped(true).show(ui, |ui| {
                        ui.label(RichText::new("Key").strong());
                        ui.label(RichText::new("Value").strong());
                        ui.end_row();

                        for (k, v) in &state.terms {
                            if !filter.is_empty()
                                && !k.to_ascii_lowercase().contains(&filter)
                                && !v.to_ascii_lowercase().contains(&filter)
                            {
                                continue;
                            }
                            ui.label(k);
                            ui.label(v);
                            ui.end_row();
                        }
                    });
                });
            });
        });
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Log Viewer Dialog
// ---------------------------------------------------------------------------

pub struct LogDialogState {
    pub filter: String,
    pub level_filter: String,
}

impl Default for LogDialogState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            level_filter: "ALL".into(),
        }
    }
}

pub fn show_log_dialog(
    ctx: &egui::Context,
    state: &mut LogDialogState,
    log_service: &LogAccessService,
    settings: &Settings,
    open: &mut bool,
) {
    let mut is_open = *open;
    Window::new(locales::get(&settings.language, "LogDialogTitle"))
        .open(&mut is_open)
        .default_width(750.0)
        .default_height(450.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut state.filter);

                ui.label("Level:");
                egui::ComboBox::from_id_salt("log_level_filter")
                    .selected_text(&state.level_filter)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.level_filter, "ALL".into(), "ALL");
                        ui.selectable_value(&mut state.level_filter, "INFO".into(), "INFO");
                        ui.selectable_value(&mut state.level_filter, "WARN".into(), "WARN");
                        ui.selectable_value(&mut state.level_filter, "ERROR".into(), "ERROR");
                    });

                if ui.button("Clear Logs").clicked() {
                    log_service.clear();
                }

                if ui.button("Open Log Folder").clicked() {
                    let _ = ShellOpenService::open(&AppPaths::log_directory().to_string_lossy());
                }
            });

            ui.add_space(8.0);
            ui.separator();

            let filter_lower = state.filter.to_ascii_lowercase();
            let logs = log_service.snapshot();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("log_grid")
                        .striped(true)
                        .num_columns(3)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Timestamp").strong());
                            ui.label(RichText::new("Level").strong());
                            ui.label(RichText::new("Message").strong());
                            ui.end_row();

                            for item in logs.iter().rev() {
                                if state.level_filter != "ALL"
                                    && !item.level.eq_ignore_ascii_case(&state.level_filter)
                                {
                                    continue;
                                }
                                if !filter_lower.is_empty()
                                    && !item.message.to_ascii_lowercase().contains(&filter_lower)
                                {
                                    continue;
                                }

                                ui.label(&item.timestamp);
                                let color = match item.level.to_ascii_uppercase().as_str() {
                                    "ERROR" | "FATAL" => Color32::RED,
                                    "WARN" | "WARNING" => Color32::YELLOW,
                                    _ => Color32::LIGHT_GRAY,
                                };
                                ui.colored_label(color, &item.level);
                                ui.label(&item.message);
                                ui.end_row();
                            }
                        });
                });
        });
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Translation Dialog (Single & Batch)
// ---------------------------------------------------------------------------

pub struct TranslateDialogState {
    is_batch: bool,
    source_lang: String,
    target_lang: String,
    current_index: usize,
    translated_text: String,
    preview_source: Option<(usize, W3StringItem)>,
    status_message: Option<String>,
    job: Option<TranslationJob>,
}

struct TranslationJob {
    task: tokio::task::JoinHandle<()>,
    result: std::sync::mpsc::Receiver<TranslationResult>,
}

impl Drop for TranslationJob {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum TranslationResult {
    Single(usize, W3StringItem, Result<String, String>),
    Batch(Vec<W3StringItem>, Vec<(usize, String)>, Option<String>),
}

impl Default for TranslateDialogState {
    fn default() -> Self {
        Self {
            is_batch: false,
            source_lang: "en".into(),
            target_lang: "pl".into(),
            current_index: 0,
            translated_text: String::new(),
            preview_source: None,
            status_message: None,
            job: None,
        }
    }
}

impl TranslateDialogState {
    fn start(
        &mut self,
        ctx: &egui::Context,
        items: &[W3StringItem],
        client: &TranslationClient,
        provider: &str,
    ) {
        let source = self.source_lang.clone();
        let target = self.target_lang.clone();
        let provider = provider.to_owned();
        let ctx = ctx.clone();
        let client = client.clone();
        let index = self.current_index;
        let batch = self.is_batch;
        let snapshot = if batch { items.to_vec() } else { vec![items[index].clone()] };
        self.preview_source = None;
        self.translated_text.clear();
        self.status_message = Some("Translating…".into());
        let (send, result) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            let result = if batch {
                let mut translations = Vec::new();
                let mut error = None;
                for (index, item) in snapshot.iter().enumerate() {
                    if item.text.trim().is_empty() {
                        continue;
                    }
                    match client.translate(&provider, &item.text, &source, &target).await {
                        Ok(text) => translations.push((index, text)),
                        Err(err) => {
                            error = Some(err);
                            break;
                        }
                    }
                }
                TranslationResult::Batch(snapshot, translations, error)
            } else {
                let item = snapshot.into_iter().next().expect("single item snapshot");
                let result = client.translate(&provider, &item.text, &source, &target).await;
                TranslationResult::Single(index, item, result)
            };
            let _ = send.send(result);
            ctx.request_repaint();
        });
        self.job = Some(TranslationJob { task, result });
    }

    fn poll(&mut self, items: &mut [W3StringItem]) {
        let Some(job) = &self.job else { return };
        let result = job.result.try_recv();
        if matches!(result, Err(std::sync::mpsc::TryRecvError::Empty)) {
            return;
        }
        self.job = None;
        self.status_message = match result {
            Ok(TranslationResult::Single(index, source, result))
                if index == self.current_index && items.get(index) == Some(&source) =>
            {
                match result {
                    Ok(text) => {
                        self.translated_text = text;
                        self.preview_source = Some((index, source));
                        None
                    }
                    Err(err) => Some(format!("Translation failed: {err}")),
                }
            }
            Ok(TranslationResult::Batch(snapshot, translations, error)) if items == snapshot => {
                let updated = translations.len();
                for (index, text) in translations {
                    apply_translation(&mut items[index], text);
                }
                Some(match error {
                    Some(err) => format!("Batch stopped after {updated} translations: {err}"),
                    None => format!("Translated {updated}/{} items.", items.len()),
                })
            }
            Ok(_) => Some("The source changed. Start the translation again.".into()),
            Err(_) => Some("Translation stopped unexpectedly.".into()),
        };
    }
}

fn apply_translation(item: &mut W3StringItem, text: String) {
    if item.text != text {
        if item.old_text.is_empty() {
            item.old_text = item.text.clone();
        }
        item.text = text;
    }
}

pub fn show_translate_dialog(
    ctx: &egui::Context,
    state: &mut TranslateDialogState,
    items: &mut [W3StringItem],
    client: &TranslationClient,
    settings: &Settings,
    open: &mut bool,
) {
    state.poll(items);
    let mut is_open = *open;
    Window::new(locales::get(&settings.language, "TranslationDialogTitle"))
        .open(&mut is_open)
        .default_width(600.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            if let Some(msg) = &state.status_message {
                ui.label(msg);
            }
            if state.job.is_some() {
                if ui.button("Cancel translation").clicked() {
                    state.job = None;
                    state.status_message = Some("Translation canceled.".into());
                }
                return;
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.is_batch, false, "Single Item Mode");
                ui.selectable_value(&mut state.is_batch, true, "Batch Items Mode");
            });
            ui.horizontal(|ui| {
                ui.label("Source Language:");
                ui.text_edit_singleline(&mut state.source_lang);
                ui.label("Target Language:");
                ui.text_edit_singleline(&mut state.target_lang);
                ui.label(format!("Provider: {}", settings.translator));
            });
            ui.separator();

            if items.is_empty() {
                ui.label("No items loaded to translate.");
                return;
            }
            if state.is_batch {
                ui.label(format!("Ready to translate {} items.", items.len()));
                if ui.button("Translate All Items").clicked() {
                    state.start(ctx, items, client, &settings.translator);
                }
                return;
            }

            state.current_index = state.current_index.min(items.len() - 1);
            let index = state.current_index;
            let item = &mut items[index];
            if state.preview_source.as_ref().is_some_and(|(i, source)| *i != index || source != item) {
                state.preview_source = None;
                state.translated_text.clear();
            }
            ui.label(format!("Item ID: {} | Key: {}", item.str_id, item.key_name));
            ui.label(RichText::new("Source Text:").strong());
            ui.add(egui::TextEdit::multiline(&mut item.text.as_str()));
            ui.label(RichText::new("Translated Text:").strong());
            if ui.text_edit_multiline(&mut state.translated_text).changed() {
                state.preview_source = Some((index, item.clone()));
            }

            ui.horizontal(|ui| {
                if ui.button("Translate").clicked() {
                    state.start(ctx, items, client, &settings.translator);
                } else if ui.add_enabled(state.preview_source.is_some(), egui::Button::new("Apply to Item")).clicked() {
                    apply_translation(&mut items[index], state.translated_text.clone());
                    state.preview_source = None;
                    state.translated_text.clear();
                    state.status_message = Some("Updated item text.".into());
                }

                let previous = ui.button("< Prev").clicked() && index > 0;
                ui.label(format!("{}/{}", index + 1, items.len()));
                let next = ui.button("Next >").clicked() && index + 1 < items.len();
                if previous || next {
                    state.current_index = if previous { index - 1 } else { index + 1 };
                    state.preview_source = None;
                    state.translated_text.clear();
                    state.status_message = None;
                }
            });
        });
    if !is_open && state.job.take().is_some() {
        state.status_message = None;
    }
    *open = is_open;
}

// ---------------------------------------------------------------------------
// Edit / Add Data Dialog
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct EditDialogState {
    pub is_add: bool,
    pub str_id: String,
    pub key_name: String,
    pub key_hex: String,
    pub text: String,
    pub target_index: Option<usize>,
}

impl EditDialogState {
    pub fn for_add(suggested_id: String) -> Self {
        Self {
            is_add: true,
            str_id: suggested_id,
            key_name: String::new(),
            key_hex: "0x00000000".into(),
            text: String::new(),
            target_index: None,
        }
    }

    pub fn for_edit(index: usize, item: &W3StringItem) -> Self {
        Self {
            is_add: false,
            str_id: item.str_id.clone(),
            key_name: item.key_name.clone(),
            key_hex: item.key_hex.clone(),
            text: item.text.clone(),
            target_index: Some(index),
        }
    }
}

pub fn show_edit_dialog(
    ctx: &egui::Context,
    state: &mut Option<EditDialogState>,
    items: &mut Vec<W3StringItem>,
    settings: &Settings,
) {
    let mut close = false;
    if let Some(st) = state {
        let title_key = if st.is_add { "AddDialogTitle" } else { "EditDialogTitle" };
        Window::new(locales::get(&settings.language, title_key))
            .resizable(false)
            .collapsible(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("String ID:").strong());
                    ui.text_edit_singleline(&mut st.str_id);
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Key Name:").strong());
                    if ui.text_edit_singleline(&mut st.key_name).changed() {
                        let hash = codec::localization_key_hash(&st.key_name);
                        st.key_hex = format!("0x{hash:08X}");
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Key (Hex):").strong());
                    ui.label(&st.key_hex);
                });

                ui.add_space(4.0);

                ui.label(RichText::new("Text:").strong());
                ui.text_edit_multiline(&mut st.text);

                ui.add_space(16.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        if st.is_add {
                            items.push(W3StringItem {
                                str_id: st.str_id.clone(),
                                key_hex: st.key_hex.clone(),
                                key_name: st.key_name.clone(),
                                old_text: String::new(),
                                text: st.text.clone(),
                            });
                        } else if let Some(idx) = st.target_index
                            && idx < items.len()
                        {
                            if items[idx].old_text.is_empty() && items[idx].text != st.text {
                                items[idx].old_text = items[idx].text.clone();
                            }
                            items[idx].str_id = st.str_id.clone();
                            items[idx].key_hex = st.key_hex.clone();
                            items[idx].key_name = st.key_name.clone();
                            items[idx].text = st.text.clone();
                        }
                        close = true;
                    }

                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
    }

    if close {
        *state = None;
    }
}

// ---------------------------------------------------------------------------
// About Dialog
// ---------------------------------------------------------------------------

pub fn show_about_dialog(
    ctx: &egui::Context,
    settings: &Settings,
    open: &mut bool,
) {
    let mut is_open = *open;
    let mut close = false;
    Window::new(locales::get(&settings.language, "AboutDialogTitle"))
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .default_width(450.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("The Witcher 3 String Editor");
            ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
            ui.label("Architecture: x86_64 Linux");
            ui.label("Framework: egui / eframe (Pure Rust Port)");
            ui.label("Port Parity: Byte-Exact Codec & Settings PARITY");

            ui.add_space(8.0);
            ui.separator();

            ui.label(RichText::new("Active Rust Units:").strong());
            ui.label("• w3codec-rs: v162 / v164 w3strings & CSV serializer");
            ui.label("• settings-rs: System.Text.Json exact round trip");
            ui.label("• xlsx-rs: calamine reader + rust_xlsxwriter formatter");
            ui.label("• dictionary-rs: Aho-Corasick dynamic term replacement");
            ui.label("• locales-rs: 12 game localization bundles");
            ui.label("• services-rs: Backup, Recent, Logging & Shell services");
            ui.label("• translate-rs: Google, Bing/Microsoft & Yandex translation");
            ui.label("• nexus-rs: NexusMods GraphQL update service");

            ui.add_space(12.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    if close {
        is_open = false;
    }
    *open = is_open;
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_rejects_multiline_text_before_preparing_overwrite() {
        let item = W3StringItem {
            str_id: "42".into(),
            text: "first\nsecond".into(),
            ..W3StringItem::empty()
        };
        let state = SaveDialogState::new(&settings_rs::port::default_settings(), Path::new("."));
        let err = save_items(
            Path::new("unused.csv"),
            &[item],
            &state,
            codec::language_from_code("en").unwrap(),
            || panic!("should reject incompatible data before touching the destination"),
        ).unwrap_err();
        assert!(err.contains("CSV cannot preserve line breaks"));
    }

    #[test]
    fn overwrite_failure_leaves_original_bytes_intact() {
        let dir = std::env::temp_dir().join(format!(
            "w3-backup-test-{}", std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("en.csv");
        std::fs::write(&path, b"original").unwrap();
        let state = SaveDialogState::new(&settings_rs::port::default_settings(), &dir);
        let result = save_items(
            &path,
            &[W3StringItem::empty()],
            &state,
            codec::language_from_code("en").unwrap(),
            || Err("backup failed".into()),
        );
        let saved = std::fs::read(&path);
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(result.unwrap_err(), "backup failed");
        assert_eq!(saved.unwrap(), b"original");
    }
}
