fn main() {
    let mut state = serial_capture::mcp::McpState::from_env();
    if let Err(err) =
        serial_capture::mcp::serve(std::io::stdin().lock(), std::io::stdout(), &mut state)
    {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
