//! **Launch-parameter deep link: a launcher that opens one Jellyfin item.**
//!
//! webOS hands a native app the `params` of a `luna://com.webos.applicationManager/launch` call as
//! one JSON string in `argv[1]` (`src/main.c` passes it to [`nj_launch_params`]). A launcher shelf
//! such as CleanYourTV's *Jellyfin continue watching* card sends `{"itemId": "<GUID>", "serverId":
//! "<GUID>"}` plus keys this app does not read (`tmdbId`, `mediaType`, `title`).
//!
//! The link is **only ever a detail page**: it never starts playback, never signs anyone in and
//! never switches the user or server. It waits on Home — behind a sign-in, the who's-watching
//! picker or a first-run question the person still has to answer — and drops after [`WAIT_MS`], so
//! a link that arrived while someone was typing a password does not yank them elsewhere a minute
//! later. A `serverId` that names another server than the one signed in is dropped for the same
//! reason a wrong item id would be: opening something of another library is worse than Home.
//!
//! Both ids cross a trust boundary (any app can call `launch`), so they are accepted only as a
//! GUID spelling — hex digits and dashes — and nothing else reaches the catalog or the log.
use super::bridge::{AppHost, Bridge};
use crate::ui::dispatch::Dispatcher;
use crate::screens::registry::AppArg;
use std::sync::Mutex;

/// How long a link waits for Home before it is dropped.
const WAIT_MS: u32 = 45_000;
/// A launch parameter longer than this is not one of ours.
const MAX_PARAMS: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeepLink {
    /// Normalized (lower-case, undashed) Jellyfin item id.
    pub(crate) item: String,
    /// Normalized id of the server the launcher read the item from, when it said.
    pub(crate) server: Option<String>,
}

/// A link armed at boot. `deadline` is the loop tick past which it is dropped, set on the first
/// frame because boot has no loop clock yet.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    link: DeepLink,
    deadline: Option<u32>,
}

static LAUNCH: Mutex<Option<DeepLink>> = Mutex::new(None);

/// A Jellyfin GUID in any of its spellings, normalized; `None` for anything else.
fn guid(s: &str) -> Option<String> {
    if !s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return None;
    }
    let n = crate::jf::ids::normalize(s);
    (n.len() == 32).then_some(n)
}

/// The link in a launch-parameter string, or `None` when it carries no valid `itemId`.
pub(crate) fn parse(params: &str) -> Option<DeepLink> {
    if params.len() > MAX_PARAMS {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(params).ok()?;
    let item = guid(v.get("itemId")?.as_str()?)?;
    let server = match v.get("serverId") {
        None | Some(serde_json::Value::Null) => None,
        Some(s) => Some(guid(s.as_str()?)?),
    };
    Some(DeepLink { item, server })
}

/// The C shim's hand-over of `argv[1]`. Anything that is not a deep link is ignored: the sole
/// other launch argument is Sentry's envelope path, which `main.c` has already claimed.
#[no_mangle]
pub extern "C" fn nj_launch_params(params: *const std::os::raw::c_char) {
    if params.is_null() {
        return;
    }
    // SAFETY: `main.c` passes a NUL-terminated `argv` entry that outlives this call.
    let s = unsafe { std::ffi::CStr::from_ptr(params) }.to_string_lossy();
    if let Some(link) = parse(&s) {
        nj_base::eventlog::log("deep link: launched with an item");
        if let Ok(mut g) = LAUNCH.lock() {
            *g = Some(link);
        }
    }
}

/// The link this process was launched with, once (the boot arms it into `App`).
pub(crate) fn take_launch() -> Option<Pending> {
    let link = LAUNCH.lock().ok()?.take()?;
    Some(Pending { link, deadline: None })
}

/// What one frame should do about an armed link.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Drop(&'static str),
    Open,
}

fn step(p: &Pending, deadline: u32, now: u32, on_free_home: bool, signed_in_server: Option<&str>) -> Step {
    if now.wrapping_sub(deadline) < u32::MAX / 2 {
        return Step::Drop("never reached Home");
    }
    if !on_free_home {
        return Step::Wait;
    }
    match (&p.link.server, signed_in_server) {
        (Some(want), Some(have)) if !have.is_empty() && crate::jf::ids::normalize(have) != *want => {
            Step::Drop("another server is signed in")
        }
        _ => Step::Open,
    }
}

/// Run every frame beside `menu_play_tick`: open the armed link's detail page once Home is up.
pub(crate) fn tick(slot: &mut Option<Pending>, pages: &mut Dispatcher<AppHost>, bridge: &mut Bridge, now: u32) {
    let Some(p) = slot.as_mut() else { return };
    let deadline = *p.deadline.get_or_insert(now.wrapping_add(WAIT_MS));
    let on_free_home = matches!(pages.top_arg(), None | Some(AppArg::Home))
        && pages.nav.top_page().is_some_and(|e| pages.nav.input_owner() == Some(nj_machine::machine::InputOwner::Entry(e.id)));
    let stored = crate::jf::store::current();
    let server = stored.as_ref().map(|s| s.server_id.as_str());
    match step(p, deadline, now, on_free_home && stored.is_some(), server) {
        Step::Wait => {}
        Step::Drop(why) => {
            nj_base::eventlog::log(&format!("deep link: dropped ({why})"));
            *slot = None;
        }
        Step::Open => {
            let rk = crate::jf::ids::rating_key(&p.link.item);
            *slot = None;
            nj_base::eventlog::log("deep link: opening the item's detail page");
            super::bridge::open_detail(pages, bridge, crate::catalog::current_server(), &rk, None, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "36edf81507b5c8eda566778b2a316c29";
    const B: &str = "0123456789abcdef0123456789abcdef";

    fn pending(server: Option<&str>) -> Pending {
        Pending { link: DeepLink { item: A.into(), server: server.map(Into::into) }, deadline: Some(1000) }
    }

    #[test]
    fn parses_every_guid_spelling_and_ignores_the_launcher_extras() {
        let want = Some(DeepLink { item: A.into(), server: Some(B.into()) });
        let dashed = "36EDF815-07B5-C8ED-A566-778B2A316C29";
        let json = format!(r#"{{"itemId":"{dashed}","serverId":"{B}","tmdbId":603,"mediaType":"movie","title":"x"}}"#);
        assert_eq!(parse(&json), want);
        assert_eq!(parse(&format!(r#"{{"itemId":"{A}"}}"#)), Some(DeepLink { item: A.into(), server: None }));
    }

    #[test]
    fn refuses_anything_that_is_not_a_guid_or_not_json() {
        for bad in [
            "", "not json", "{}", r#"{"itemId":42}"#, r#"{"itemId":""}"#, r#"{"itemId":"abc"}"#,
            r#"{"itemId":"../../etc/passwd"}"#, r#"{"itemId":"36edf81507b5c8eda566778b2a316c2z"}"#,
            r#"{"itemId":"36edf81507b5c8eda566778b2a316c29","serverId":"nope"}"#,
            r#"{"itemId":"36edf81507b5c8eda566778b2a316c29","serverId":7}"#,
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
        let long = format!(r#"{{"itemId":"{A}","pad":"{}"}}"#, "x".repeat(MAX_PARAMS));
        assert_eq!(parse(&long), None);
    }

    #[test]
    fn waits_for_a_free_home_then_opens() {
        let p = pending(None);
        assert_eq!(step(&p, 1000, 0, false, Some("")), Step::Wait);
        assert_eq!(step(&p, 1000, 0, true, Some("")), Step::Open);
    }

    #[test]
    fn drops_when_the_wait_is_over_even_off_home() {
        let p = pending(None);
        assert_eq!(step(&p, 1000, 999, false, None), Step::Wait);
        assert!(matches!(step(&p, 1000, 1001, false, None), Step::Drop(_)));
        assert!(matches!(step(&p, 1000, 1001, true, None), Step::Drop(_)));
    }

    #[test]
    fn another_server_is_dropped_but_an_unnamed_or_unknown_one_is_not() {
        let p = pending(Some(B));
        assert!(matches!(step(&p, 1000, 0, true, Some(A)), Step::Drop(_)));
        assert_eq!(step(&p, 1000, 0, true, Some(&B.to_uppercase())), Step::Open);
        assert_eq!(step(&p, 1000, 0, true, Some("")), Step::Open, "a server that never reported its id");
        assert_eq!(step(&pending(None), 1000, 0, true, Some(A)), Step::Open);
    }
}
