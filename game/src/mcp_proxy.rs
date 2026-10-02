//! Connect a stdio MCP client to the already running Katla editor.
#[cfg(unix)]
fn main() -> std::io::Result<()> {
    use std::os::unix::net::UnixStream;
    let path = std::env::args_os()
        .nth(1)
        .or_else(|| std::env::var_os("KATLA_MCP_SOCKET"))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Pass the editor socket path or set KATLA_MCP_SOCKET",
            )
        })?;
    let mut socket = UnixStream::connect(path)?;
    let mut reader = socket.try_clone()?;
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut socket);
        let _ = socket.shutdown(std::net::Shutdown::Write);
    });
    std::io::copy(&mut reader, &mut std::io::stdout().lock())?;
    Ok(())
}
#[cfg(not(unix))]
fn main() {
    eprintln!("Editor socket attachment requires Unix.");
    std::process::exit(1);
}
