//! Supervisor: cria o socketpair privilegiado, lança o shell como filho e
//! garante o ciclo de vida (fail-secure de sessão). O privilégio é decidido
//! pela ponta do socketpair inserida com `is_shell=true` — nunca por PID/env
//! (AGENTS.md §3.5). Se o compositor morre, o kernel mata os filhos (PDEATHSIG).

use std::ffi::OsStr;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use rustix::io::{fcntl_setfd, FdFlags};
use rustix::process::{
    getpid, getppid, kill_process, set_parent_process_death_signal, Pid, Signal,
};
use smithay::reexports::wayland_server::DisplayHandle;

use crate::state::ClientState;

/// Filhos lançados pelo supervisor, rastreados para o shutdown gracioso.
pub struct Supervisor {
    children: Vec<Child>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    /// Lança o shell privilegiado: socketpair interno, ponta do compositor
    /// inserida como `is_shell=true`, ponta do filho entregue via
    /// `WAYLAND_SOCKET` (nome reservado às bibliotecas cliente).
    pub fn spawn_shell(
        &mut self,
        display_handle: &mut DisplayHandle,
        shell_path: &str,
        shell_args: &[String],
        shell_env: &[String],
        socket_name: &OsStr,
    ) -> io::Result<()> {
        let (server, client) = UnixStream::pair()?;
        display_handle
            .insert_client(
                server,
                Arc::new(ClientState {
                    is_shell: true,
                    ..Default::default()
                }),
            )
            .map_err(|err| io::Error::other(err.to_string()))?;
        let mut cmd = Command::new(shell_path);
        cmd.args(shell_args);
        for entry in shell_env {
            if let Some((key, value)) = entry.split_once('=') {
                cmd.env(key, value);
            }
        }
        configure_privileged(&mut cmd, &client, socket_name);
        let pid = self.spawn(&mut cmd)?;
        tracing::info!(
            pid,
            shell_path,
            "shell privilegiado lançado pelo supervisor"
        );
        Ok(())
    }

    /// Lança um filho rastreado. O chamador configura o `Command`.
    pub fn spawn(&mut self, cmd: &mut Command) -> io::Result<u32> {
        let child = cmd.spawn()?;
        let pid = child.id();
        self.children.push(child);
        Ok(pid)
    }

    /// Shutdown gracioso: SIGTERM, espera curta, SIGKILL e reap de cada filho.
    pub fn shutdown(&mut self) {
        for child in &self.children {
            let _ = kill_process(Pid::from_child(child), Signal::TERM);
        }
        for child in &mut self.children {
            let pid = Pid::from_child(child);
            for _ in 0..50 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = kill_process(pid, Signal::KILL);
            let _ = child.wait();
        }
        self.children.clear();
    }
}

/// Aplica o env isolado e as garantias de ciclo de vida a um `Command`
/// privilegiado. A ponta do socketpair (`client`) sobrevive ao exec apenas
/// neste filho; clientes comuns nunca a herdam.
pub fn configure_privileged(cmd: &mut Command, client: &UnixStream, socket_name: &OsStr) {
    let raw_fd = client.as_raw_fd();
    let parent_pid = getpid();
    cmd.env("WAYLAND_DISPLAY", socket_name)
        .env("WAYLAND_SOCKET", raw_fd.to_string())
        .env("GDK_BACKEND", "wayland")
        .env_remove("DISPLAY")
        .env_remove("AERO_PRIVILEGED_FD");
    // SAFETY: `pre_exec` roda no filho após fork e antes do exec, em contexto
    // async-signal-safe. Só usamos syscalls async-signal-safe (getppid, prctl,
    // fcntl); o caminho de erro não aloca (os_error é empacotado, sem heap).
    unsafe {
        cmd.pre_exec(move || {
            parent_death_guard(parent_pid)?;
            // SAFETY: `raw_fd` foi herdado no fork e segue aberto no filho até
            // o exec; limpamos CLOEXEC para a biblioteca cliente recebê-lo.
            let fd = BorrowedFd::borrow_raw(raw_fd);
            fcntl_setfd(fd, FdFlags::empty())
                .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))?;
            Ok(())
        });
    }
}

/// Ciclo de vida para filho NÃO privilegiado (cliente público `--command`):
/// PDEATHSIG na morte do compositor, sem socket privilegiado nem env de shell.
pub fn configure_lifecycle(cmd: &mut Command) {
    let parent_pid = getpid();
    // SAFETY: `pre_exec` roda no filho após fork e antes do exec; só syscalls
    // async-signal-safe (getppid, prctl) e o caminho de erro não aloca.
    unsafe {
        cmd.pre_exec(move || parent_death_guard(parent_pid));
    }
}

/// Guarda async-signal-safe contra a corrida fork/PDEATHSIG: se o pai já morreu
/// antes de armar o sinal, aborta o filho (Err faz o std dar `_exit`); senão arma
/// SIGKILL para o kernel matar o filho quando o compositor morrer.
fn parent_death_guard(parent_pid: Pid) -> io::Result<()> {
    if getppid() != Some(parent_pid) {
        return Err(io::Error::from_raw_os_error(3)); // ESRCH
    }
    set_parent_process_death_signal(Some(Signal::KILL))
        .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn proc_alive(pid: u32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[test]
    fn privileged_child_inherits_socket_fd() {
        let (server, client) = UnixStream::pair().unwrap();
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("test -e /proc/self/fd/$WAYLAND_SOCKET");
        configure_privileged(&mut cmd, &client, OsStr::new("wayland-test"));
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "fd do socketpair não sobreviveu ao exec: {out:?}"
        );
        drop(server);
    }

    #[test]
    fn privileged_child_env_is_isolated() {
        let (_server, client) = UnixStream::pair().unwrap();
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg("printf '%s|%s' \"$DISPLAY\" \"$AERO_PRIVILEGED_FD\"");
        cmd.env("DISPLAY", ":0").env("AERO_PRIVILEGED_FD", "99");
        configure_privileged(&mut cmd, &client, OsStr::new("wayland-test"));
        let out = cmd.output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "|");
    }

    #[test]
    fn shutdown_terminates_and_reaps_children() {
        let mut sup = Supervisor::new();
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("30");
        let pid = sup.spawn(&mut cmd).unwrap();
        assert!(
            proc_alive(pid),
            "filho deveria estar vivo antes do shutdown"
        );
        sup.shutdown();
        assert!(
            !proc_alive(pid),
            "filho {pid} deveria ter sido terminado e reapado"
        );
    }
}
