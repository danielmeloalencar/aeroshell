//! Shim de compatibilidade: o frame real vive em `surfaces::frame` no host.

fn main() {
    aeroshell_shell::surfaces::frame::main();
}
