//! HBR audio support for rooted TVs: runs `hbr/nativejelly-hbr.sh` (shipped beside the app) as root
//! through Homebrew Channel's `exec` service, which a rooted set exposes to apps on the `public`
//! LS2 group. Blocking: call it from a worker thread. See `vendor/earc_route/README.md`.

#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
const EXEC: &str = "luna://org.webosbrew.hbchannel.service/exec";

/// The installer's one result line (`hbr: active`, `hbr: removed`, `hbr: REFUSED: …`), or why the
/// service could not run it (no Homebrew Channel, not rooted, no answer).
#[cfg_attr(any(feature = "hostsim", feature = "test-support"), allow(dead_code))]
fn parse(reply: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(reply).map_err(|_| "unreadable reply".to_string())?;
    if value.get("returnValue").and_then(serde_json::Value::as_bool) != Some(true) {
        let why = value.get("errorText").and_then(serde_json::Value::as_str).unwrap_or("refused");
        return Err(format!("Homebrew Channel: {why}"));
    }
    let out = value.get("stdoutString").and_then(serde_json::Value::as_str).unwrap_or("");
    Ok(out.lines().last().unwrap_or("").trim().to_string())
}

#[cfg(any(feature = "hostsim", test, feature = "test-support"))]
pub fn run(_action: &str) -> Result<String, String> {
    Err("not a television".into())
}

/// Runs `nativejelly-hbr.sh <action>`: `install` (copies it beside the boot hooks and loads it),
/// `remove`, or `status`.
#[cfg(all(not(feature = "hostsim"), not(any(test, feature = "test-support"))))]
pub fn run(action: &str) -> Result<String, String> {
    use super::ls2;
    const BUDGET: std::time::Duration = std::time::Duration::from_secs(60);
    let dir = nj_base::paths::app_dir().display().to_string();
    let command = match action {
        "install" => format!("sh '{dir}/hbr/nativejelly-hbr.sh' install '{dir}'"),
        "remove" | "status" => format!("sh '{dir}/hbr/nativejelly-hbr.sh' {action}"),
        _ => return Err(format!("unknown action {action}")),
    };
    let payload = serde_json::json!({ "command": command }).to_string();
    let registration = ls2::register().map_err(|e| format!("bus: {e}"))?;
    let reply = registration.call(EXEC, &payload, BUDGET).map_err(|_| "Homebrew Channel did not answer".to_string())?;
    let line = parse(&reply);
    nj_base::eventlog::log(&format!("hbr: {action}: {line:?}"));
    line
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn the_installer_line_is_the_answer() {
        let reply = r#"{"returnValue":true,"stdoutString":"hbr: active\n","stdoutBytes":"aGJyOiBhY3RpdmUK","stderrString":"","stderrBytes":""}"#;
        assert_eq!(parse(reply), Ok("hbr: active".into()));
    }

    #[test]
    fn a_refused_call_is_an_error() {
        assert!(parse(r#"{"returnValue":false,"errorText":"Denied method call"}"#).is_err());
    }
}
