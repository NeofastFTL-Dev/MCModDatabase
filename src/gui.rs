use crate::api::{self, key, ModHit};
use crate::cache::Library;
use crate::catalog;
use crate::compat::{self, PackReport};
use crate::github;
use crate::index;
use crate::prompt;
use crate::settings::Settings;
use crate::update::{self, Release};
use crate::util::{self, fmt_count};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ACCENT: Color32 = Color32::from_rgb(62, 224, 160);
const DIM: Color32 = Color32::from_rgb(141, 151, 168);
const BG: Color32 = Color32::from_rgb(8, 9, 12);
const WARN: Color32 = Color32::from_rgb(232, 184, 74);
const ERR: Color32 = Color32::from_rgb(240, 113, 120);
const PINK: Color32 = Color32::from_rgb(245, 169, 184);
const SKY: Color32 = Color32::from_rgb(91, 206, 250);
const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Search,
    Catalog,
    Library,
    Saved,
    Settings,
    Support,
}

enum Msg {
    SearchDone {
        gen: u64,
        hits: Vec<ModHit>,
        total: u64,
        warnings: Vec<String>,
    },
    Versions(Vec<String>),
    LoaderVersions(Vec<String>),
    Pack(PackReport),
    DlProgress(String),
    DlDone(String, Option<PathBuf>),
    PageLoaded {
        key: String,
        page: api::ModPage,
        github: Option<github::Repo>,
        issues: Vec<github::GhIssue>,
        hints: Vec<github::CompatHint>,
        error: String,
    },
    UpdateAvailable(Release),
    UpdateProgress(String),
    UpdateFailed(String),
    UpdateReady(PathBuf),
}

struct DescWin {
    hit: ModHit,
    open: bool,
    loading: bool,
    page: api::ModPage,
    github: Option<github::Repo>,
    issues: Vec<github::GhIssue>,
    hints: Vec<github::CompatHint>,
    error: String,
}

fn take_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub struct App {
    settings: Settings,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    library: Library,
    view: View,
    query: String,
    source: String,
    project_type: String,
    loader: String,
    version: String,
    loader_version: String,
    sort: String,
    last_edit: Option<Instant>,
    hits: Vec<ModHit>,
    total: u64,
    warnings: Vec<String>,
    loading: bool,
    versions: Vec<String>,
    loader_versions: Vec<String>,
    selected: HashMap<String, ModHit>,
    pack: Option<PackReport>,
    status: String,
    cf_key_edit: String,
    focus: Option<String>,
    prompt_note: String,
    search_gen: u64,
    index_tx: Sender<index::Event>,
    index_rx: Receiver<index::Event>,
    catalog_stats: catalog::Stats,
    catalog_themes: Vec<catalog::ThemeCount>,
    catalog_theme: String,
    index_status: String,
    index_running: bool,
    desc_windows: HashMap<String, DescWin>,
    update: Option<Release>,
    update_open: bool,
    updating: bool,
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = Color32::from_rgb(16, 19, 26);
    v.extreme_bg_color = Color32::from_rgb(12, 16, 22);
    v.faint_bg_color = Color32::from_rgb(23, 28, 38);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgb(26, 77, 58);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.window_rounding = Rounding::same(10.0);
    v.widgets.inactive.bg_fill = Color32::from_rgb(23, 28, 38);
    v.widgets.hovered.bg_fill = Color32::from_rgb(36, 48, 42);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.bg_fill = Color32::from_rgb(30, 90, 68);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.interaction.selectable_labels = false;
    });
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dir = PathBuf::from(&windir).join("Fonts");
    let extras = [
        ("ui", dir.join("segoeui.ttf")),
        ("ui", PathBuf::from("/System/Library/Fonts/Supplemental/Arial.ttf")),
        ("sym", dir.join("seguisym.ttf")),
        ("emoji", dir.join("seguiemj.ttf")),
        ("cjk", dir.join("msyh.ttc")),
        ("cjk-kr", dir.join("malgun.ttf")),
        ("cjk-jp", dir.join("YuGothM.ttc")),
    ];
    let mut loaded = Vec::new();
    for (name, path) in extras {
        if loaded.iter().any(|n| *n == name) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            fonts
                .font_data
                .insert(name.into(), egui::FontData::from_owned(bytes));
            loaded.push(name);
        }
    }
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        for name in loaded.iter().rev() {
            list.insert(0, (*name).to_string());
        }
    }
    ctx.set_fonts(fonts);
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        setup_fonts(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let (index_tx, index_rx) = mpsc::channel();
        let settings = Settings::load();
        let cf_key_edit = settings.curseforge_key.clone();
        let catalog_stats = catalog::stats();
        let catalog_themes = catalog::theme_counts();
        let should_index = catalog::should_auto_index();
        let app = App {
            settings,
            tx: tx.clone(),
            rx,
            ctx: cc.egui_ctx.clone(),
            library: Library::load(),
            view: View::Search,
            query: String::new(),
            source: "all".into(),
            project_type: "mod".into(),
            loader: String::new(),
            version: String::new(),
            loader_version: String::new(),
            sort: "relevance".into(),
            last_edit: Some(Instant::now()),
            hits: Vec::new(),
            total: 0,
            warnings: Vec::new(),
            loading: false,
            versions: Vec::new(),
            loader_versions: Vec::new(),
            selected: HashMap::new(),
            pack: None,
            status: "Ready".into(),
            cf_key_edit,
            focus: None,
            prompt_note: String::new(),
            search_gen: 0,
            index_tx,
            index_rx,
            catalog_stats,
            catalog_themes,
            catalog_theme: String::new(),
            index_status: if should_index {
                "Building catalog…".into()
            } else {
                String::new()
            },
            index_running: should_index,
            desc_windows: HashMap::new(),
            update: None,
            update_open: false,
            updating: false,
        };
        let txv = tx.clone();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let list = api::game_versions().unwrap_or_default();
            let _ = txv.send(Msg::Versions(list));
            ctx.request_repaint();
        });
        if should_index {
            index::start(
                app.index_tx.clone(),
                cc.egui_ctx.clone(),
                app.settings.curseforge_key.clone(),
                false,
            );
        }
        app.kick_update_check();
        app
    }

    fn start_index(&mut self, force: bool) {
        if index::is_running() {
            self.index_status = "Catalog index already running.".into();
            return;
        }
        if !force && !catalog::can_refresh(false) {
            self.index_status = "Catalog is fresh (under 20 hours). Use Force refresh to rebuild.".into();
            return;
        }
        self.index_running = true;
        self.index_status = "Starting catalog index…".into();
        index::start(
            self.index_tx.clone(),
            self.ctx.clone(),
            self.settings.curseforge_key.clone(),
            force,
        );
    }

    fn kick_update_check(&self) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if let Ok(Some(rel)) = update::latest() {
                let _ = tx.send(Msg::UpdateAvailable(rel));
                ctx.request_repaint();
            }
        });
    }

    fn start_app_update(&mut self) {
        let Some(rel) = self.update.clone() else {
            return;
        };
        self.updating = true;
        self.status = "Downloading update…".into();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::UpdateProgress(format!("Downloading {}…", rel.asset_name)));
            ctx.request_repaint();
            match update::download_installer(&rel) {
                Ok(path) => {
                    let _ = tx.send(Msg::UpdateReady(path));
                }
                Err(e) => {
                    let _ = tx.send(Msg::UpdateFailed(format!("Update failed: {e}")));
                }
            }
            ctx.request_repaint();
        });
    }

    fn load_catalog_hits(&mut self) {
        let q = self.query.trim().to_string();
        let source = self.source.clone();
        let loader = self.loader.clone();
        let version = self.version.clone();
        let ptype = self.project_type.clone();
        let theme = self.catalog_theme.clone();
        let writing = catalog::is_writing() || index::is_running();
        let loaded = catch_unwind(AssertUnwindSafe(|| {
            let stats = catalog::stats();
            let themes = if writing {
                Vec::new()
            } else {
                catalog::theme_counts()
            };
            let hits = if !q.is_empty() && !prompt::is_description(&prompt::parse(&q), &q) {
                catalog::fts_search(&q, &source, &loader, &version, &ptype, 80)
            } else {
                catalog::mods_for_theme(&theme, &source, &loader, &version, &ptype, 80)
            };
            (stats, themes, hits)
        }));
        match loaded {
            Ok((stats, themes, hits)) => {
                if stats.total > 0 {
                    self.catalog_stats = stats;
                }
                if !themes.is_empty() {
                    self.catalog_themes = themes;
                }
                if !hits.is_empty() || !writing {
                    self.hits = hits;
                }
                self.total = self.hits.len() as u64;
                self.loading = false;
                self.status = if writing {
                    format!("{} catalog mods (updating…)", self.total)
                } else {
                    format!("{} catalog mods", self.total)
                };
            }
            Err(_) => {
                self.loading = false;
                self.status = "Catalog is updating…".into();
            }
        }
    }

    fn kick_search(&mut self) {
        if self.view == View::Catalog {
            self.load_catalog_hits();
            return;
        }
        self.search_gen = self.search_gen.wrapping_add(1);
        let gen = self.search_gen;
        let plan = prompt::parse(&self.query);
        if self.version.is_empty() && !plan.version.is_empty() {
            self.version = plan.version.clone();
            self.kick_loader_versions();
        }
        if self.loader.is_empty() && !plan.loader.is_empty() {
            self.loader = plan.loader.clone();
            self.kick_loader_versions();
        }
        if !plan.project_type.is_empty() {
            self.project_type = plan.project_type.clone();
        }
        self.prompt_note = prompt::summary(&plan);
        self.loading = true;
        self.status = if prompt::is_description(&plan, &self.query) {
            format!("Finding mods for: {}", self.prompt_note)
        } else {
            "Searching…".into()
        };
        let q = self.query.clone();
        let source = if self.view == View::Library {
            "local".into()
        } else {
            self.source.clone()
        };
        let ptype = self.project_type.clone();
        let loader = self.loader.clone();
        let version = self.version.clone();
        let sort = self.sort.clone();
        let cf = self.settings.curseforge_key.clone();
        let fav_only = self.view == View::Saved;
        let local = self.view != View::Search;
        let described = prompt::is_description(&plan, &q);
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let lib_hits = if local {
            let hits = self
                .library
                .search(&q, &source, &loader, &version, &ptype, fav_only);
            if described {
                prompt::rank_local(hits, &plan)
            } else {
                hits
            }
        } else {
            Vec::new()
        };
        std::thread::spawn(move || {
            if local {
                let total = lib_hits.len() as u64;
                let _ = tx.send(Msg::SearchDone {
                    gen,
                    hits: lib_hits,
                    total,
                    warnings: Vec::new(),
                });
            } else if described {
                let (hits, total, warnings) = prompt::run(&plan, &source, &ptype, &loader, &version, &cf);
                let _ = tx.send(Msg::SearchDone {
                    gen,
                    hits,
                    total,
                    warnings,
                });
            } else {
                let local = catalog::fts_search(&q, &source, &loader, &version, &ptype, 40);
                if q.is_empty() || local.len() < 10 {
                    let (hits, total, warnings) =
                        api::search(&q, &source, &ptype, &loader, &version, &sort, 0, 40, &cf);
                    let _ = tx.send(Msg::SearchDone {
                        gen,
                        hits,
                        total,
                        warnings,
                    });
                } else {
                    let total = local.len() as u64;
                    let _ = tx.send(Msg::SearchDone {
                        gen,
                        hits: local,
                        total,
                        warnings: vec!["Results from local catalog.".into()],
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    fn kick_loader_versions(&mut self) {
        let loader = self.loader.clone();
        let version = self.version.clone();
        if loader.is_empty() {
            self.loader_versions.clear();
            return;
        }
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let list = api::loader_versions(&loader, &version);
            let _ = tx.send(Msg::LoaderVersions(list));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::SearchDone {
                    gen,
                    hits,
                    total,
                    warnings,
                } => {
                    if gen != self.search_gen {
                        continue;
                    }
                    self.library.upsert(&hits);
                    self.hits = hits;
                    self.total = total;
                    self.warnings = warnings;
                    self.loading = false;
                    self.status = format!("{} results", self.total);
                }
                Msg::Versions(v) => self.versions = v,
                Msg::LoaderVersions(v) => self.loader_versions = v,
                Msg::Pack(p) => {
                    self.loading = false;
                    self.status = if p.ok {
                        "No blocking incompatibilities".into()
                    } else {
                        "Compatibility problems found".into()
                    };
                    self.pack = Some(p);
                }
                Msg::DlProgress(s) => self.status = s,
                Msg::DlDone(s, path) => {
                    self.loading = false;
                    self.status = s;
                    if let Some(p) = path {
                        util::open_in_explorer(&p);
                    }
                }
                Msg::PageLoaded {
                    key,
                    page,
                    github,
                    issues,
                    hints,
                    error,
                } => {
                    if let Some(w) = self.desc_windows.get_mut(&key) {
                        w.page = page;
                        if github.is_some() {
                            w.github = github;
                        }
                        if !issues.is_empty() {
                            w.issues = issues;
                        }
                        if !hints.is_empty() {
                            w.hints = hints;
                        }
                        w.error = error;
                        w.loading = false;
                    }
                }
                Msg::UpdateAvailable(rel) => {
                    let skip = self.settings.skip_update == rel.tag;
                    self.update = Some(rel);
                    self.update_open = !skip;
                }
                Msg::UpdateProgress(s) => self.status = s,
                Msg::UpdateFailed(s) => {
                    self.updating = false;
                    self.status = s;
                }
                Msg::UpdateReady(path) => {
                    self.status = "Installing update… MCDB will restart.".into();
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(600));
                        let _ = update::launch_installer(&path);
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        while let Ok(ev) = self.index_rx.try_recv() {
            match ev {
                index::Event::Progress(p) => {
                    self.index_running = true;
                    self.index_status = format!("{}  ({} MR · {} CF)", p.message, p.mr, p.cf);
                    self.catalog_stats.modrinth = p.mr;
                    self.catalog_stats.curseforge = p.cf;
                    self.catalog_stats.total = p.mr + p.cf;
                }
                index::Event::Done(s) => {
                    self.index_running = false;
                    self.index_status = s.clone();
                    self.status = s;
                    self.catalog_stats = catalog::stats();
                    self.catalog_themes = catalog::theme_counts();
                    if self.view == View::Catalog {
                        self.load_catalog_hits();
                    }
                }
            }
        }
        if let Some(t) = self.last_edit {
            if t.elapsed() >= DEBOUNCE && self.view != View::Settings && self.view != View::Support {
                let plan = prompt::parse(&self.query);
                self.prompt_note = prompt::summary(&plan);
                if self.loading {
                    return;
                }
                self.last_edit = None;
                if self.view == View::Catalog {
                    self.load_catalog_hits();
                } else if prompt::is_description(&plan, &self.query) {
                    self.status = "Press Find to search from your description".into();
                } else {
                    self.kick_search();
                }
            }
        }
    }

    fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[(&str, &str)]) -> bool {
        let mut changed = false;
        let label = items
            .iter()
            .find(|(v, _)| *v == value.as_str())
            .map(|(_, l)| *l)
            .unwrap_or(value.as_str());
        egui::ComboBox::from_id_salt(id)
            .selected_text(label.to_string())
            .show_ui(ui, |ui| {
                for (v, l) in items {
                    if ui.selectable_label(*value == *v, *l).clicked() {
                        *value = (*v).to_string();
                        changed = true;
                    }
                }
            });
        changed
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("ModDB").heading().color(ACCENT));
        ui.label(RichText::new("CurseForge + Modrinth").small().color(DIM));
        ui.add_space(10.0);
        for (view, label) in [
            (View::Search, "Search"),
            (View::Catalog, "Catalog"),
            (View::Library, "Library"),
            (View::Saved, "Saved"),
            (View::Settings, "Settings"),
            (View::Support, "Support ♡"),
        ] {
            if ui
                .selectable_label(self.view == view, label)
                .clicked()
            {
                self.view = view;
                if view == View::Catalog {
                    self.load_catalog_hits();
                } else if view != View::Settings && view != View::Support {
                    self.last_edit = Some(Instant::now());
                }
            }
        }
        ui.separator();
        ui.label(RichText::new("Source").small().color(DIM));
        if Self::combo(
            ui,
            "source",
            &mut self.source,
            &[
                ("all", "Both sites"),
                ("modrinth", "Modrinth"),
                ("curseforge", "CurseForge"),
                ("local", "Local cache"),
            ],
        ) {
            self.last_edit = Some(Instant::now());
        }
        ui.label(RichText::new("Type").small().color(DIM));
        if Self::combo(
            ui,
            "type",
            &mut self.project_type,
            &[
                ("mod", "Mods"),
                ("modpack", "Modpacks"),
                ("resourcepack", "Resource packs"),
                ("shader", "Shaders"),
            ],
        ) {
            self.last_edit = Some(Instant::now());
        }
        ui.label(RichText::new("Loader").small().color(DIM));
        if Self::combo(
            ui,
            "loader",
            &mut self.loader,
            &[
                ("", "Any loader"),
                ("fabric", "Fabric"),
                ("forge", "Forge"),
                ("neoforge", "NeoForge"),
                ("quilt", "Quilt"),
            ],
        ) {
            self.kick_loader_versions();
            self.last_edit = Some(Instant::now());
        }
        ui.label(RichText::new("Minecraft").small().color(DIM));
        let ver_before = self.version.clone();
        ui.add(egui::TextEdit::singleline(&mut self.version).hint_text("e.g. 1.21.1"));
        if self.version != ver_before {
            self.kick_loader_versions();
            self.last_edit = Some(Instant::now());
        }
        if !self.versions.is_empty() {
            ui.add_enabled_ui(true, |ui| {
                egui::ComboBox::from_id_salt("mcver")
                    .selected_text(if self.version.is_empty() {
                        "pick version".into()
                    } else {
                        self.version.clone()
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.version.is_empty(), "Any").clicked() {
                            self.version.clear();
                            self.last_edit = Some(Instant::now());
                        }
                        let versions = self.versions.iter().take(80).cloned().collect::<Vec<_>>();
                        for v in versions {
                            if ui.selectable_label(self.version == v, &v).clicked() {
                                self.version = v;
                                self.kick_loader_versions();
                                self.last_edit = Some(Instant::now());
                            }
                        }
                    });
            });
        }
        ui.label(RichText::new("Loader version").small().color(DIM));
        ui.add_enabled(
            !self.loader.is_empty(),
            egui::TextEdit::singleline(&mut self.loader_version).hint_text("e.g. 0.16.14"),
        );
        if !self.loader_versions.is_empty() {
            egui::ComboBox::from_id_salt("lver")
                .selected_text(if self.loader_version.is_empty() {
                    "pick loader ver".into()
                } else {
                    self.loader_version.clone()
                })
                .show_ui(ui, |ui| {
                    let versions = self.loader_versions.clone();
                    for v in versions {
                        if ui.selectable_label(self.loader_version == v, &v).clicked() {
                            self.loader_version = v;
                        }
                    }
                });
        }
        ui.label(RichText::new("Sort").small().color(DIM));
        if Self::combo(
            ui,
            "sort",
            &mut self.sort,
            &[
                ("relevance", "Relevance"),
                ("downloads", "Downloads"),
                ("updated", "Updated"),
                ("newest", "Newest"),
                ("follows", "Follows"),
                ("name", "Name"),
            ],
        ) {
            self.last_edit = Some(Instant::now());
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            let (t, mr, cf, fav) = self.library.stats();
            ui.label(
                RichText::new(format!("{t} cached\n{mr} Modrinth · {cf} CurseForge\n{fav} saved"))
                    .small()
                    .color(DIM),
            );
        });
    }

    fn results_table(&mut self, ui: &mut egui::Ui) {
        let mut toggle: Option<ModHit> = None;
        let mut fav: Option<ModHit> = None;
        let mut open: Option<String> = None;
        let mut open_desc: Option<ModHit> = None;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::exact(28.0))
            .column(Column::remainder().at_least(160.0))
            .column(Column::initial(120.0))
            .column(Column::initial(90.0))
            .column(Column::initial(110.0))
            .column(Column::initial(90.0))
            .column(Column::exact(28.0))
            .header(22.0, |mut h| {
                h.col(|_| {});
                h.col(|ui| {
                    ui.label("Name");
                });
                h.col(|ui| {
                    ui.label("Author");
                });
                h.col(|ui| {
                    ui.label("Source");
                });
                h.col(|ui| {
                    ui.label("Loader / MC");
                });
                h.col(|ui| {
                    ui.label("Downloads");
                });
                h.col(|_| {});
            })
            .body(|mut body| {
                for m in &self.hits {
                    let k = key(m);
                    let on = self.selected.contains_key(&k);
                    let starred = self.library.is_favorite(m);
                    body.row(26.0, |mut row| {
                        row.col(|ui| {
                            let mut checked = on;
                            if ui.checkbox(&mut checked, "").changed() {
                                toggle = Some(m.clone());
                            }
                        });
                        row.col(|ui| {
                            ui.label(&m.name);
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&m.author).color(DIM));
                        });
                        row.col(|ui| {
                            ui.label(&m.source);
                        });
                        row.col(|ui| {
                            let loaders = m.loaders.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
                            let ver = m.game_versions.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
                            ui.label(RichText::new(format!("{loaders}  {ver}")).small().color(DIM));
                        });
                        row.col(|ui| {
                            ui.label(fmt_count(m.downloads));
                        });
                        row.col(|ui| {
                            let star = if starred { "★" } else { "☆" };
                            if ui.button(star).clicked() {
                                fav = Some(m.clone());
                            }
                        });
                        if row.response().clicked() {
                            self.focus = Some(k.clone());
                        }
                        if row.response().double_clicked() {
                            open = Some(m.website_url.clone());
                        }
                        if row.response().secondary_clicked() {
                            open_desc = Some(m.clone());
                        }
                        row.response().context_menu(|ui| {
                            if ui.button("View description").clicked() {
                                open_desc = Some(m.clone());
                                ui.close_menu();
                            }
                            if ui.button("Open on site").clicked() {
                                open = Some(m.website_url.clone());
                                ui.close_menu();
                            }
                        });
                    });
                }
            });
        if let Some(m) = toggle {
            let k = key(&m);
            if self.selected.remove(&k).is_none() {
                self.selected.insert(k, m);
            }
            self.pack = None;
        }
        if let Some(m) = fav {
            self.library.toggle_favorite(&m);
        }
        if let Some(url) = open {
            util::open_url(&url);
        }
        if let Some(m) = open_desc {
            self.open_description(m);
        }
    }

    fn open_description(&mut self, m: ModHit) {
        let k = key(&m);
        if let Some(w) = self.desc_windows.get_mut(&k) {
            w.open = true;
            return;
        }
        let page = github::load_page(&m.source, &m.source_id).unwrap_or_else(|| api::ModPage {
            body: m.summary.clone(),
            ..Default::default()
        });
        let gh = github::load_repo(&m.source, &m.source_id);
        let issues = github::load_issues(&m.source, &m.source_id);
        let hints = github::load_hints(&m.source, &m.source_id);
        self.desc_windows.insert(
            k.clone(),
            DescWin {
                hit: m.clone(),
                open: true,
                loading: true,
                page,
                github: gh,
                issues,
                hints,
                error: String::new(),
            },
        );
        let cf = self.settings.curseforge_key.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let (page, error) = match api::fetch_mod_page(&m, &cf) {
                Ok(p) => (p, String::new()),
                Err(e) => (
                    api::ModPage {
                        body: m.summary.clone(),
                        ..Default::default()
                    },
                    e.to_string(),
                ),
            };
            github::save_page(&m.source, &m.source_id, &page);
            let snap = github::ingest(&m.source, &m.source_id, &page, &[m.website_url.as_str()]);
            let (github, issues, hints) = match snap {
                Some(s) => (Some(s.repo), s.issues, s.hints),
                None => (None, Vec::new(), Vec::new()),
            };
            let _ = tx.send(Msg::PageLoaded {
                key: k,
                page,
                github,
                issues,
                hints,
                error,
            });
            ctx.request_repaint();
        });
    }

    fn show_desc_windows(&mut self, ctx: &egui::Context) {
        let mut close = Vec::new();
        for (i, (k, win)) in self.desc_windows.iter_mut().enumerate() {
            let mut open = win.open;
            let offset = 24.0 * i as f32;
            let title = format!("{} — description", win.hit.name);
            egui::Window::new(title)
                .id(egui::Id::new(("modpage", k.as_str())))
                .open(&mut open)
                .collapsible(true)
                .resizable(true)
                .default_size([520.0, 560.0])
                .min_size([320.0, 220.0])
                .default_pos(egui::pos2(260.0 + offset, 72.0 + offset))
                .show(ctx, |ui| {
                    if win.loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading description…");
                        });
                    }
                    if !win.error.is_empty() {
                        ui.label(RichText::new(&win.error).color(ERR));
                    }
                    ui.label(RichText::new(&win.hit.author).color(DIM));
                    ui.horizontal_wrapped(|ui| {
                        ui.hyperlink_to("Open on site", &win.hit.website_url);
                        if !win.page.source_url.is_empty() {
                            ui.hyperlink_to("Source", &win.page.source_url);
                        }
                        if !win.page.issues_url.is_empty() {
                            ui.hyperlink_to("Issues", &win.page.issues_url);
                        }
                        if !win.page.wiki_url.is_empty() {
                            ui.hyperlink_to("Wiki", &win.page.wiki_url);
                        }
                    });
                    if let Some(gh) = &win.github {
                        ui.separator();
                        ui.label(
                            RichText::new(format!("GitHub  {}/{}", gh.owner, gh.repo))
                                .strong()
                                .color(ACCENT),
                        );
                        let license = if gh.license.is_empty() {
                            String::new()
                        } else {
                            format!("   {}", gh.license)
                        };
                        ui.label(format!(
                            "★ {}   forks {}   {} issues {}{}",
                            fmt_count(gh.stars),
                            fmt_count(gh.forks),
                            gh.open_issues,
                            gh.language,
                            license
                        ));
                        ui.hyperlink_to(&gh.html_url, &gh.html_url);
                        if !gh.description.is_empty() {
                            ui.label(&gh.description);
                        }
                        if !gh.topics.is_empty() {
                            ui.label(RichText::new(&gh.topics).small().color(DIM));
                        }
                    }
                    if !win.hints.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Compat from GitHub issues").strong());
                        for h in win.hints.iter().take(12) {
                            let color = if h.severity == "warn" { WARN } else { DIM };
                            ui.label(
                                RichText::new(format!("{} · {} — {}", h.kind, h.target, h.detail))
                                    .small()
                                    .color(color),
                            );
                        }
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::rich::render(ui, take_bytes(&win.page.body, 80_000));
                            if !win.issues.is_empty() {
                                ui.add_space(12.0);
                                ui.label(RichText::new("GitHub issues").strong());
                                for it in win.issues.iter().take(25) {
                                    ui.horizontal_wrapped(|ui| {
                                        let color = if it.state == "open" { WARN } else { DIM };
                                        ui.label(
                                            RichText::new(format!("#{} {}", it.number, it.state))
                                                .small()
                                                .color(color),
                                        );
                                        if it.html_url.is_empty() {
                                            ui.label(&it.title);
                                        } else {
                                            ui.hyperlink_to(&it.title, &it.html_url);
                                        }
                                    });
                                    if !it.labels.is_empty() {
                                        ui.label(RichText::new(&it.labels).small().color(DIM));
                                    }
                                }
                            }
                            if let Some(gh) = &win.github {
                                if !gh.readme.is_empty() {
                                    ui.add_space(12.0);
                                    ui.label(RichText::new("README").strong());
                                    crate::rich::render(ui, take_bytes(&gh.readme, 40_000));
                                }
                            }
                        });
                });
            win.open = open;
            if !win.open {
                close.push(k.clone());
            }
        }
        for k in close {
            self.desc_windows.remove(&k);
        }
    }

    fn pack_bar(&mut self, ui: &mut egui::Ui) {
        let n = self.selected.len();
        if n == 0 {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{n} mods selected")).strong());
            let target = [&self.loader, &self.loader_version, &self.version]
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ");
            if !target.is_empty() {
                ui.label(RichText::new(target).color(DIM));
            }
            if ui.button("Check compatibility").clicked() {
                self.run_check();
            }
            if ui.button("Download to folder").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.start_download(dir, false);
                }
            }
            if ui.button("Save zip").clicked() {
                let mut dlg = rfd::FileDialog::new().add_filter("zip", &["zip"]);
                dlg = dlg.set_file_name("mods.zip");
                if let Some(path) = dlg.save_file() {
                    self.start_download(path, true);
                }
            }
            if ui.button("Clear").clicked() {
                self.selected.clear();
                self.pack = None;
            }
        });
        if let Some(pack) = &self.pack {
            for issue in &pack.issues {
                let color = match issue.severity.as_str() {
                    "error" => ERR,
                    "warn" => WARN,
                    _ => DIM,
                };
                ui.label(RichText::new(&issue.message).color(color));
            }
            if pack.ok {
                ui.label(RichText::new("No blocking incompatibilities.").color(ACCENT));
            }
            for f in pack.files.iter().take(12) {
                ui.label(RichText::new(format!("• {}  ({})", f.name, f.file_name)).small().color(DIM));
            }
        }
    }

    fn run_check(&mut self) {
        let mods: Vec<ModHit> = self.selected.values().cloned().collect();
        let gv = self.version.clone();
        let loader = self.loader.clone();
        let cf = self.settings.curseforge_key.clone();
        self.loading = true;
        self.status = "Checking compatibility…".into();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let report = compat::check_pack(&mods, &gv, &loader, &cf);
            let _ = tx.send(Msg::Pack(report));
            ctx.request_repaint();
        });
    }

    fn start_download(&mut self, dest: PathBuf, as_zip: bool) {
        let mods: Vec<ModHit> = self.selected.values().cloned().collect();
        if mods.is_empty() {
            return;
        }
        let pack = self.pack.clone();
        let gv = self.version.clone();
        let loader = self.loader.clone();
        let cf = self.settings.curseforge_key.clone();
        self.loading = true;
        self.status = "Preparing download…".into();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let report = match pack {
                Some(p) if !p.files.is_empty() => p,
                _ => compat::check_pack(&mods, &gv, &loader, &cf),
            };
            if report.files.is_empty() {
                let _ = tx.send(Msg::DlDone("No matching files to download.".into(), None));
                ctx.request_repaint();
                return;
            }
            let staging = if as_zip {
                std::env::temp_dir().join(format!("mcdb-dl-{}", std::process::id()))
            } else {
                dest.clone()
            };
            let _ = std::fs::create_dir_all(&staging);
            let mut ok = 0usize;
            let mut failed = Vec::new();
            let mut saved: Vec<(String, PathBuf)> = Vec::new();
            let of = report.files.len();
            for (n, f) in report.files.iter().enumerate() {
                let _ = tx.send(Msg::DlProgress(format!("Downloading {} ({}/{of})", f.name, n + 1)));
                ctx.request_repaint();
                if f.url.is_empty() {
                    failed.push(f.name.clone());
                    continue;
                }
                let dest_file = staging.join(util::sanitize_component(&f.file_name));
                let extra = if f.source == "curseforge" && !cf.is_empty() {
                    vec![("x-api-key", cf.as_str())]
                } else {
                    vec![]
                };
                match crate::net::download_file(&f.url, &extra, &dest_file, |_, _| {}) {
                    Ok(_) => {
                        ok += 1;
                        saved.push((util::sanitize_component(&f.file_name), dest_file));
                    }
                    Err(_) => failed.push(f.name.clone()),
                }
            }
            let open_path;
            if as_zip {
                match util::zip_files(&dest, &saved) {
                    Ok(()) => {
                        for (_, p) in &saved {
                            let _ = std::fs::remove_file(p);
                        }
                        let _ = std::fs::remove_dir(&staging);
                        open_path = dest.parent().map(|p| p.to_path_buf()).or(Some(dest.clone()));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::DlDone(format!("Zip failed: {e}"), Some(staging)));
                        ctx.request_repaint();
                        return;
                    }
                }
            } else {
                open_path = Some(staging);
            }
            let msg = if failed.is_empty() {
                if as_zip {
                    format!("Saved zip with {ok} files")
                } else {
                    format!("Downloaded {ok} files")
                }
            } else {
                format!("Downloaded {ok}, failed: {}", failed.join(", "))
            };
            let _ = tx.send(Msg::DlDone(msg, open_path));
            ctx.request_repaint();
        });
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("Modrinth is public. CurseForge needs an API key from console.curseforge.com");
        ui.add_space(8.0);
        ui.label("CurseForge API key");
        ui.add(egui::TextEdit::singleline(&mut self.cf_key_edit).password(true));
        ui.horizontal(|ui| {
            if ui.button("Save key").clicked() {
                self.settings.curseforge_key = self.cf_key_edit.trim().to_string();
                self.settings.save();
                self.status = if self.settings.curseforge_key.is_empty() {
                    "CurseForge key cleared".into()
                } else {
                    "CurseForge key saved".into()
                };
            }
        });
        ui.add_space(12.0);
        ui.label(format!("Download folder: {}", self.settings.download_dir.display()));
        if ui.button("Choose folder").clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.settings.download_dir = dir;
                self.settings.save();
            }
        }
        if ui.button("Open folder").clicked() {
            let _ = std::fs::create_dir_all(&self.settings.download_dir);
            util::open_in_explorer(&self.settings.download_dir);
        }
        ui.add_space(16.0);
        ui.heading("Catalog");
        ui.label(format!(
            "{} mods  ({} Modrinth · {} CurseForge)",
            self.catalog_stats.total, self.catalog_stats.modrinth, self.catalog_stats.curseforge
        ));
        if !self.index_status.is_empty() {
            ui.label(RichText::new(&self.index_status).color(DIM));
        }
        if self.index_running {
            ui.spinner();
        }
        ui.horizontal(|ui| {
            if ui.button("Refresh catalog").clicked() {
                self.start_index(false);
            }
            if ui.button("Force refresh").clicked() {
                self.start_index(true);
            }
        });
        ui.label(RichText::new("Caches the top ~10k projects per source and type by downloads. Live Search still queries the APIs. At most once per 20 hours unless forced.").small().color(DIM));
        ui.add_space(16.0);
        ui.heading("GitHub");
        let gs = github::stats();
        ui.label(format!(
            "{} repos  ·  {} issues  ·  {} compat hints",
            gs.repos, gs.issues, gs.hints
        ));
        ui.label(RichText::new("Right-click a mod to fetch its page. If it has a GitHub repo, issues are stored and used for compatibility checks.").small().color(DIM));
        ui.add_space(16.0);
        ui.heading("Updates");
        ui.label(format!("This build: {}", update::current()));
        if ui.button("Check for updates").clicked() {
            self.status = "Checking GitHub releases…".into();
            self.kick_update_check();
        }
    }

    fn support_panel(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        let stripe_h = 10.0;
        let colors = [SKY, PINK, Color32::from_rgb(255, 255, 255), PINK, SKY];
        let w = full.width() / colors.len() as f32;
        for (i, c) in colors.iter().enumerate() {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(full.min.x + i as f32 * w, full.min.y),
                    egui::vec2(w, stripe_h),
                ),
                0.0,
                *c,
            );
        }
        ui.add_space(18.0);
        ui.label(RichText::new("hey, I'm NeofastFTL ♡").heading().color(PINK));
        ui.label(RichText::new("transfem · she/her · I built MCDB so finding mods feels a little softer").color(SKY));
        ui.add_space(10.0);
        ui.label("Thank you for using my little Minecraft mod browser. I hope it saves you a few tabs and a bit of chaos.");
        ui.add_space(8.0);
        ui.label(RichText::new("If MCDB helped you, a GitHub star is the sweetest thank-you.").color(PINK));
        ui.horizontal(|ui| {
            if ui.button("Open GitHub ♡").clicked() {
                util::open_url("https://github.com/NeofastFTL-Dev/MCModDatabase");
            }
            if ui.button("Releases").clicked() {
                util::open_url("https://github.com/NeofastFTL-Dev/MCModDatabase/releases");
            }
        });
        ui.add_space(16.0);
        ui.heading(RichText::new("Support").color(SKY));
        ui.label("Stars, issues, and kind notes keep this project alive. No pressure — just vibes.");
        if let Some(rel) = &self.update {
            ui.add_space(8.0);
            ui.label(RichText::new(format!("A new build is waiting: {}", rel.tag)).color(PINK));
            if ui.button("Download, install, restart").clicked() {
                self.start_app_update();
            }
        } else {
            ui.label(RichText::new(format!("You're on {} — I'll ping you when GitHub has a newer release.", update::current())).small().color(DIM));
            if ui.button("Check now").clicked() {
                self.kick_update_check();
            }
        }
        ui.add_space(18.0);
        ui.heading(RichText::new("ARR license").color(PINK));
        ui.label(RichText::new("© 2026 NeofastFTL — All Rights Reserved").strong());
        ui.label("The MCDB name, icon, and this app's presentation are mine. Please don't rebrand it as your own.");
        ui.label("You're warmly welcome to use it, love it, and share it with friends.");
        ui.label(RichText::new("Made with too much pink, a little sky blue, and a lot of stubborn girl energy.").italics().color(SKY));
        ui.add_space(12.0);
        ui.label(RichText::new("thank you for being here  ♡").color(PINK));
    }

    fn update_modal(&mut self, ctx: &egui::Context) {
        if !self.update_open {
            return;
        }
        let Some(rel) = self.update.clone() else {
            return;
        };
        let mut close = false;
        let mut skip = false;
        let mut go = false;
        let busy = self.updating;
        egui::Window::new("New MCDB build ♡")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} is out",
                        if rel.name.is_empty() {
                            rel.tag.clone()
                        } else {
                            rel.name.clone()
                        }
                    ))
                    .strong()
                    .color(PINK),
                );
                ui.label(
                    RichText::new(format!(
                        "You have {} · GitHub has {}",
                        update::current(),
                        rel.tag
                    ))
                    .small()
                    .color(DIM),
                );
                if !rel.notes.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&rel.notes).small());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!busy, egui::Button::new("Download, install, restart"))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Later").clicked() {
                        close = true;
                    }
                    if ui.button("Skip this version").clicked() {
                        skip = true;
                    }
                });
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Fetching the installer… then I'll close so it can replace me.");
                    });
                }
            });
        if go {
            self.start_app_update();
        }
        if skip {
            self.settings.skip_update = rel.tag;
            self.settings.save();
            close = true;
        }
        if close {
            self.update_open = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.index_running || self.last_edit.is_some() || self.loading || self.updating {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.update_modal(ctx);
        egui::SidePanel::left("side")
            .default_width(230.0)
            .show(ctx, |ui| self.sidebar(ui));
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let find = ui.add_sized([100.0, 52.0], egui::Button::new(RichText::new("Find").size(18.0).strong()));
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut self.query)
                        .desired_width(ui.available_width())
                        .desired_rows(2)
                        .hint_text("Describe the mods you want, e.g. fabric 1.21.1 optimization and light rendering plus mekanism, tech and magic"),
                );
                if resp.changed() {
                    self.last_edit = Some(Instant::now());
                    self.prompt_note = prompt::summary(&prompt::parse(&self.query));
                }
                if find.clicked()
                    || ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
                {
                    self.last_edit = None;
                    if self.view != View::Library
                        && self.view != View::Saved
                        && self.view != View::Settings
                        && self.view != View::Support
                    {
                        self.view = View::Search;
                    }
                    self.kick_search();
                }
            });
            if !self.prompt_note.is_empty() {
                ui.label(RichText::new(format!("Prompt: {}", self.prompt_note)).color(ACCENT));
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).color(DIM));
                if self.loading {
                    ui.spinner();
                }
            });
            for w in &self.warnings {
                ui.label(RichText::new(w).color(WARN));
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.view == View::Settings {
                self.settings_panel(ui);
                return;
            }
            if self.view == View::Support {
                self.support_panel(ui);
                return;
            }
            if self.view == View::Catalog {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(self.catalog_theme.is_empty(), "All")
                        .clicked()
                    {
                        self.catalog_theme.clear();
                        self.load_catalog_hits();
                    }
                    let themes = self.catalog_themes.clone();
                    for t in themes {
                        let label = format!("{} ({})", t.label, t.count);
                        if ui
                            .selectable_label(self.catalog_theme == t.id, label)
                            .clicked()
                        {
                            self.catalog_theme = t.id;
                            self.load_catalog_hits();
                        }
                    }
                });
                ui.separator();
                if self.catalog_stats.total == 0 {
                    ui.label("Catalog is empty. It builds in the background, or use Refresh catalog in Settings.");
                }
            }
            self.results_table(ui);
            self.pack_bar(ui);
            if let Some(k) = &self.focus {
                if let Some(m) = self.hits.iter().find(|x| key(x) == *k) {
                    ui.separator();
                    ui.label(RichText::new(&m.name).strong());
                    ui.label(RichText::new(&m.summary).color(DIM));
                    ui.hyperlink_to("Open on site", &m.website_url);
                }
            }
        });
        self.show_desc_windows(ctx);
        if self.desc_windows.values().any(|w| w.loading) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.settings.save();
        self.library.save();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.settings.save();
        self.library.save();
    }
}

pub fn run_gui() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1180.0, 720.0])
        .with_min_inner_size([860.0, 520.0])
        .with_title("ModDB");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native("ModDB", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
