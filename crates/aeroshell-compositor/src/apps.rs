//! Aplicativos `.desktop` do launcher (F2-launcher).
//!
//! O compositor oferece apenas o mecanismo: descoberta, identidade e spawn.
//! Busca e apresentação ficam no SDK/painel. Nenhuma dependência de desktop
//! nova é necessária para o subconjunto de Desktop Entry usado aqui.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use aeroshell_ipc::types::App;

#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub info: App,
    startup_wm_class: Option<String>,
}

impl InstalledApp {
    /// Casa a identidade do xdg_toplevel sem depender de PID ou privilégio.
    /// Além do ID completo, aceita o último segmento (`org.gnome.gedit` →
    /// `gedit`), StartupWMClass e o basename do executável.
    pub fn matches_window_app_id(&self, window_app_id: &str) -> bool {
        let candidate = window_app_id.trim();
        if candidate.is_empty() {
            return false;
        }
        let short_id = self.info.id.rsplit('.').next().unwrap_or(&self.info.id);
        let executable = self
            .info
            .exec
            .split_whitespace()
            .next()
            .map(|value| value.trim_matches('"'))
            .and_then(|value| Path::new(value).file_name())
            .and_then(|value| value.to_str());
        [
            Some(self.info.id.as_str()),
            Some(short_id),
            self.startup_wm_class.as_deref(),
            executable,
        ]
        .into_iter()
        .flatten()
        .any(|value| value.eq_ignore_ascii_case(candidate))
    }

    #[cfg(test)]
    fn for_test(id: &str, name: &str, exec: &str, startup_wm_class: &str) -> Self {
        Self {
            info: App {
                id: id.to_owned(),
                name: name.to_owned(),
                icon: String::new(),
                exec: exec.to_owned(),
                categories: Vec::new(),
            },
            startup_wm_class: Some(startup_wm_class.to_owned()),
        }
    }
}

/// Carrega sistema primeiro e usuário depois: uma entrada do usuário com o
/// mesmo ID substitui a do sistema, como manda a precedência dos diretórios.
pub fn load_apps() -> Vec<InstalledApp> {
    load_apps_from_dirs(&app_dirs())
}

fn app_dirs() -> Vec<PathBuf> {
    // Fixture determinística dos harnesses; nunca é lida no binário sem bench.
    #[cfg(feature = "bench")]
    if let Some(value) = std::env::var_os("AERO_APP_DIRS") {
        return std::env::split_paths(&value).collect();
    }

    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs
}

fn load_apps_from_dirs(dirs: &[PathBuf]) -> Vec<InstalledApp> {
    let mut by_id = BTreeMap::<String, InstalledApp>::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_desktop(id, &contents) {
                by_id.insert(id.to_owned(), app);
            }
        }
    }
    let mut apps: Vec<_> = by_id.into_values().collect();
    apps.sort_by(|a, b| {
        a.info
            .name
            .to_lowercase()
            .cmp(&b.info.name.to_lowercase())
            .then_with(|| a.info.id.cmp(&b.info.id))
    });
    apps
}

fn parse_desktop(id: &str, contents: &str) -> Option<InstalledApp> {
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = String::new();
    let mut categories = Vec::new();
    let mut startup_wm_class = None;
    let mut hidden = false;
    let mut application = true;

    for raw in contents.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Type" => application = value == "Application",
            "Name" => name = Some(value.to_owned()),
            "Exec" => exec = Some(value.to_owned()),
            "Icon" => icon = value.to_owned(),
            "Categories" => {
                categories = value
                    .split(';')
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            "StartupWMClass" => startup_wm_class = Some(value.to_owned()),
            "NoDisplay" | "Hidden" => hidden |= value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if hidden || !application {
        return None;
    }
    let name = name.filter(|value| !value.is_empty())?;
    let exec = sanitize_exec(&exec?);
    if exec.is_empty() {
        return None;
    }
    Some(InstalledApp {
        info: App {
            id: id.to_owned(),
            name,
            icon,
            exec,
            categories,
        },
        startup_wm_class,
    })
}

/// Remove field codes (`%U`, `%f`, `%c`, …) sem interpretar o comando no
/// compositor. `%%` permanece como `%`; espaços dentro de aspas são preservados.
fn sanitize_exec(exec: &str) -> String {
    let mut without_codes = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            without_codes.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => without_codes.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v' | 'm') => {}
            Some(other) => {
                without_codes.push('%');
                without_codes.push(other);
            }
            None => without_codes.push('%'),
        }
    }

    let mut out = String::with_capacity(without_codes.len());
    let mut quoted = false;
    let mut pending_space = false;
    for ch in without_codes.trim().chars() {
        if ch == '"' {
            quoted = !quoted;
        }
        if ch.is_whitespace() && !quoted {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_desktop, sanitize_exec, InstalledApp};

    #[test]
    fn le_somente_desktop_entry_e_campos_publicos() {
        let app = parse_desktop(
            "org.gnome.gedit",
            "[Desktop Entry]\nType=Application\nName=gedit\nName[pt_BR]=Editor\nExec=gedit %U\nIcon=org.gnome.gedit\nCategories=GNOME;GTK;Utility;TextEditor;\nStartupWMClass=gedit\n\n[Desktop Action new-window]\nName=New Window\nExec=gedit --new-window\n",
        )
        .expect("desktop válido");
        assert_eq!(app.info.id, "org.gnome.gedit");
        assert_eq!(app.info.name, "gedit");
        assert_eq!(app.info.icon, "org.gnome.gedit");
        assert_eq!(app.info.exec, "gedit");
        assert_eq!(
            app.info.categories,
            ["GNOME", "GTK", "Utility", "TextEditor"]
        );
        assert!(app.matches_window_app_id("org.gnome.gedit"));
        assert!(app.matches_window_app_id("gedit"));
        assert!(!app.matches_window_app_id("foot"));
    }

    #[test]
    fn ignora_ocultos_e_remove_field_codes_do_exec() {
        assert!(parse_desktop(
            "oculto",
            "[Desktop Entry]\nType=Application\nName=Oculto\nExec=true\nNoDisplay=true\n",
        )
        .is_none());
        assert_eq!(sanitize_exec("gedit %U"), "gedit");
        assert_eq!(sanitize_exec("app --name %c %f %%"), "app --name %");
    }

    #[test]
    fn casa_id_sem_namespace_e_startup_wm_class() {
        let app = InstalledApp::for_test("aero.test.gedit", "gedit", "foot", "foot");
        assert!(app.matches_window_app_id("aero.test.gedit"));
        assert!(app.matches_window_app_id("gedit"));
        assert!(app.matches_window_app_id("foot"));
    }
}
