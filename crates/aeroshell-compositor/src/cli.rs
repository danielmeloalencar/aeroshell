//! CLI do compositor. `Mod` e autodetecção de backend vivem aqui; domínio não conhece backend.

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Winit,
    Headless,
    Udev,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Winit => "winit",
            Self::Headless => "headless",
            Self::Udev => "udev",
        }
    }
}

/// Abstração `Mod`: Super na sessão real, Alt no nested (configurável).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    Super,
    Alt,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub backend: BackendKind,
    pub socket: String,
    pub size: Option<(u32, u32)>,
    pub outputs: usize,
    pub scale: f64,
    pub mod_key: ModKey,
    pub command: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            backend: autodetect(),
            socket: "wayland-aero-0".to_owned(),
            size: None,
            outputs: 1,
            scale: 1.0,
            mod_key: ModKey::Alt,
            command: None,
        }
    }
}

/// Se há um compositor host (WAYLAND_DISPLAY ou DISPLAY), usar winit; senão udev.
pub fn autodetect() -> BackendKind {
    if env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some() {
        BackendKind::Winit
    } else {
        BackendKind::Udev
    }
}

pub fn parse() -> Options {
    let mut opts = Options::default();
    let mut args = env::args().skip(1).peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--backend" => {
                let value = args.next().unwrap_or_else(|| die("--backend requer um valor"));
                opts.backend = match value.as_str() {
                    "winit" | "nested" => BackendKind::Winit,
                    "headless" => BackendKind::Headless,
                    "udev" => BackendKind::Udev,
                    other => die(&format!("backend desconhecido: {other}")),
                };
            }
            "--socket" => opts.socket = args.next().unwrap_or_else(|| die("--socket requer um valor")),
            "--size" => {
                let value = args.next().unwrap_or_else(|| die("--size requer WxH"));
                let mut parts = value.split('x');
                let w = parts.next().and_then(|v| v.parse().ok());
                let h = parts.next().and_then(|v| v.parse().ok());
                match (w, h) {
                    (Some(w), Some(h)) => opts.size = Some((w, h)),
                    _ => die("--size deve ser WxH, ex.: 1280x800"),
                }
            }
            "--outputs" => {
                let value = args.next().unwrap_or_else(|| die("--outputs requer um valor"));
                opts.outputs = value.parse().unwrap_or_else(|_| die("--outputs deve ser inteiro"));
            }
            "--scale" => {
                let value = args.next().unwrap_or_else(|| die("--scale requer um valor"));
                opts.scale = value.parse().unwrap_or_else(|_| die("--scale deve ser numérico"));
            }
            "--mod" => {
                let value = args.next().unwrap_or_else(|| die("--mod requer um valor"));
                opts.mod_key = match value.as_str() {
                    "super" => ModKey::Super,
                    "alt" => ModKey::Alt,
                    other => die(&format!("mod desconhecido: {other} (use super|alt)")),
                };
            }
            "--command" | "-c" => {
                opts.command = Some(args.next().unwrap_or_else(|| die("--command requer um valor")));
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => die(&format!("argumento desconhecido: {other}\n\n{}", usage())),
        }
    }

    opts
}

pub fn usage() -> &'static str {
    "\
Uso: aeroshell-compositor [OPÇÕES]

  --backend winit|headless|udev   Backend (padrão: autodetect)
  --socket NOME                   Nome do socket Wayland (padrão: wayland-aero-0)
  --size WxH                      Tamanho da janela/nested
  --outputs N                     Número de outputs (padrão: 1)
  --scale F                       Escala fracionária (padrão: 1.0)
  --mod super|alt                 Tecla Mod (padrão: alt em nested)
  --command CMD                   Cliente inicial (env filho isolado)
"
}

fn die(message: &str) -> ! {
    eprintln!("aeroshell-compositor: {message}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_nested_friendly() {
        let opts = Options::default();
        assert_eq!(opts.socket, "wayland-aero-0");
        assert_eq!(opts.outputs, 1);
        assert_eq!(opts.scale, 1.0);
        assert_eq!(opts.mod_key, ModKey::Alt);
    }

    #[test]
    fn backend_names_are_stable() {
        assert_eq!(BackendKind::Winit.as_str(), "winit");
        assert_eq!(BackendKind::Headless.as_str(), "headless");
        assert_eq!(BackendKind::Udev.as_str(), "udev");
    }

    #[test]
    fn usage_documents_nested_flags() {
        let usage = usage();
        for flag in ["--backend", "--socket", "--size", "--outputs", "--scale", "--mod"] {
            assert!(usage.contains(flag), "falta documentar {flag}");
        }
    }
}
