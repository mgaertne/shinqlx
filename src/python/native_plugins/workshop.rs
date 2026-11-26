use alloc::collections::BTreeMap;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyList, PyTuple},
};

use super::{
    create_cvar, register_command, register_hook,
    target::{reply, usage},
};
use crate::{
    maps::loader,
    python::{
        AbstractChannel,
        commands::CommandPriorities,
        defer_delay,
        engine_api::console_command,
        events_enum::EventName,
        game::GameMethods,
        get_cvar,
        plugin::{Plugin, PluginMethods, cvar_items},
    },
    workshop::{
        item::{self, ListFiles, Pk3Maps},
        list::WorkshopList,
        steam::{self, SteamLibrary, SteamWorkshop},
    },
};

const POLL_SECONDS: f64 = 3.0;
const POLL_ATTEMPTS: u32 = 200;
const GAME_DIR: &str = "baseq3";

/// Adds the workshop items of the cvar `qlx_workshopReferences` to every map, and to the running one when it is loaded.
/// With `!workshop` admins install and remove items while the server runs.
#[pyclass(name = "workshop", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Workshop {
    /// The items that are being downloaded, with the code of the failure once the engine reported one.
    downloading: Mutex<BTreeMap<u64, Option<u32>>>,
    steam: Box<dyn SteamWorkshop>,
}

impl Workshop {
    fn new() -> Self {
        Self::with_steam(Box::new(SteamLibrary))
    }

    fn with_steam(steam: Box<dyn SteamWorkshop>) -> Self {
        Self {
            downloading: Mutex::new(BTreeMap::new()),
            steam,
        }
    }

    fn is_downloading(&self, id: u64) -> bool {
        self.downloading
            .lock()
            .is_ok_and(|downloading| downloading.contains_key(&id))
    }

    fn mark_downloading(&self, id: u64) {
        if let Ok(mut downloading) = self.downloading.lock() {
            downloading.insert(id, None);
        }
    }

    /// Ends the download of the item and returns the code of the failure, if the engine reported one.
    fn take_download(&self, id: u64) -> Option<u32> {
        self.downloading.lock().ok()?.remove(&id).flatten()
    }

    fn record_failure(&self, id: u64, code: u32) {
        if let Ok(mut downloading) = self.downloading.lock()
            && let Some(failure) = downloading.get_mut(&id)
        {
            *failure = Some(code);
        }
    }

    fn still_downloading(&self, id: u64) -> String {
        if self.steam.state(id).is_some_and(|state| state.is_queued()) {
            return format!("Item {id} is still downloading, Steam has queued it.");
        }
        self.steam
            .progress(id)
            .and_then(|progress| progress.describe())
            .map_or_else(
                || format!("Item {id} is still downloading."),
                |progress| format!("Item {id} is still downloading: {progress}."),
            )
    }
}

fn cvar_text(name: &str, default: &str) -> PyResult<String> {
    Ok(get_cvar(name)?
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned()))
}

fn list_files() -> PyResult<ListFiles> {
    let homepath = cvar_text("fs_homepath", "")?;
    let basepath = cvar_text("fs_basepath", "")?;
    let gamedir = cvar_text("fs_game", GAME_DIR)?;
    let file_name = cvar_text("sv_workshopFile", "workshop.txt")?;
    Ok(item::locate_list(
        &cvar_text("qlx_workshopFile", "")?,
        Path::new(&homepath),
        Path::new(&basepath),
        &gamedir,
        &file_name,
    ))
}

fn content_root() -> PyResult<Option<PathBuf>> {
    let homepath = PathBuf::from(cvar_text("fs_homepath", "")?);
    let basepath = PathBuf::from(cvar_text("fs_basepath", "")?);
    let working_directory = std::env::current_dir().unwrap_or_default();
    Ok(item::content_root(&[
        &basepath,
        &working_directory,
        &homepath,
    ]))
}

fn read_list(files: &ListFiles) -> io::Result<WorkshopList> {
    files
        .read
        .as_ref()
        .map_or_else(|| Ok(String::new()), fs::read_to_string)
        .map(|content| WorkshopList::parse(&content))
}

fn write_list(files: &ListFiles, list: &WorkshopList) -> io::Result<()> {
    if let Some(parent) = files.write.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&files.write, list.render())
}

/// Applies `change` to the list and writes it back when it changed; returns whether it did.
fn update_list(change: impl FnOnce(&mut WorkshopList) -> bool) -> PyResult<bool> {
    let files = list_files()?;
    let mut list = read_list(&files)?;
    let changed = change(&mut list);
    if changed {
        write_list(&files, &list)?;
    }
    Ok(changed)
}

fn installed_pk3s(root: &Path, id: u64) -> Option<Vec<Pk3Maps>> {
    item::pk3_maps(root, id)
        .ok()
        .filter(|pk3s| !pk3s.is_empty())
}

/// The warning for a server with so many `.pk3` files that the engine may become unstable, if the limit is set.
fn pk3_warning(root: &Path, list: &WorkshopList) -> PyResult<Option<String>> {
    let limit = cvar_text("qlx_workshopWarnPk3", "350")?
        .parse::<usize>()
        .unwrap_or(0);
    let total = list
        .entries()
        .map(|(id, _)| item::pk3_count(root, id))
        .sum::<usize>();
    Ok((limit > 0 && total > limit).then(|| {
        format!(
            "Warning: the workshop items bring {total} pk3 files, more than {limit}. From about 380 pk3 files the info strings of the engine overflow, from about 750 it runs out of memory when a map loads; a larger com_zoneMegs (64) helps."
        )
    }))
}

fn parse_id(text: Option<&String>) -> Option<u64> {
    text?.parse().ok().filter(|id| *id != 0)
}

#[pymethods]
impl Workshop {
    #[pyo3(signature = (*_args, **_kwargs))]
    fn handle_map(
        slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        add_references(slf)
    }

    fn cmd_workshop(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        match msg.get(1).map(|word| word.to_ascii_lowercase()).as_deref() {
            Some("add") => add_item(slf, &msg, channel),
            Some("remove") => remove_item(slf, &msg, channel),
            Some("list") => list_items(&msg, channel),
            _ => usage(),
        }
    }

    fn handle_console_print(&self, text: &str) {
        if let Some((id, code)) = steam::download_failure(text) {
            self.record_failure(id, code);
        }
    }

    fn check_download(
        slf: &Bound<'_, Self>,
        channel: &Bound<'_, AbstractChannel>,
        id: u64,
        attempts_left: u32,
    ) -> PyResult<()> {
        let failure = slf.get().take_download(id);
        let Some(root) = content_root()? else {
            return reply(channel, "The workshop directory was not found.").map(|_| ());
        };
        let pk3s = installed_pk3s(&root, id);
        if let Some(code) = failure {
            let usable = slf
                .get()
                .steam
                .state(id)
                .is_none_or(|state| state.is_installed());
            return report_failure(channel, id, code, &root, pk3s.filter(|_| usable).as_deref());
        }

        let state = slf.get().steam.state(id).unwrap_or_default();
        match pk3s {
            Some(pk3s) if !state.is_busy() && (!state.needs_update() || attempts_left == 0) => {
                finish_install(channel, id, &root, &pk3s)
            }
            None if !state.is_busy() && attempts_left == 0 => reply(
                channel,
                &format!("Item {id} did not arrive, see the server log for the download result."),
            )
            .map(|_| ()),
            _ => wait_for_download(slf, channel, id, attempts_left.saturating_sub(1)),
        }
    }
}

/// Reports a failed download; an older version that is on the disk gets installed instead.
fn report_failure(
    channel: &Bound<'_, AbstractChannel>,
    id: u64,
    code: u32,
    root: &Path,
    installed: Option<&[Pk3Maps]>,
) -> PyResult<()> {
    reply(
        channel,
        &format!("The download of item {id} failed with EResult code {code}."),
    )?;
    let Some(pk3s) = installed else {
        return Ok(());
    };
    mount(id)?;
    finish_install(channel, id, root, pk3s)
}

/// Lets the engine download the item, or mount it when it is on the disk already.
fn mount(id: u64) -> PyResult<()> {
    console_command(&format!("steam_downloadugc {id}"))
}

fn finish_install(
    channel: &Bound<'_, AbstractChannel>,
    id: u64,
    root: &Path,
    pk3s: &[Pk3Maps],
) -> PyResult<()> {
    let description = item::describe(pk3s);
    update_list(|list| list.add(id, &description))?;
    loader::request_refresh();
    reply(channel, &format!("Item {id} is installed: {description}"))?;

    let files = list_files()?;
    if let Some(warning) = pk3_warning(root, &read_list(&files)?)? {
        reply(channel, &warning)?;
    }
    Ok(())
}

fn wait_for_download(
    slf: &Bound<'_, Workshop>,
    channel: &Bound<'_, AbstractChannel>,
    id: u64,
    attempts_left: u32,
) -> PyResult<()> {
    defer_delay(
        &slf.getattr(intern!(slf.py(), "check_download"))?,
        POLL_SECONDS,
        (channel, id, attempts_left),
    )?;
    slf.get().mark_downloading(id);
    Ok(())
}

fn add_item(
    slf: &Bound<'_, Workshop>,
    msg: &[String],
    channel: &Bound<'_, AbstractChannel>,
) -> PyResult<Option<i32>> {
    let Some(id) = parse_id(msg.get(2)) else {
        return usage();
    };
    if slf.get().is_downloading(id) {
        return reply(channel, &slf.get().still_downloading(id));
    }
    let Some(root) = content_root()? else {
        return reply(channel, "The workshop directory was not found.");
    };

    let cached = installed_pk3s(&root, id);
    // Steam is asked directly: the engine neither updates an installed item nor fetches a deleted one again
    if !slf.get().steam.download(id) {
        mount(id)?;
        if let Some(pk3s) = &cached {
            finish_install(channel, id, &root, pk3s)?;
            return Ok(None);
        }
    }
    let text = if cached.is_some() {
        format!("Item {id} is on the disk, checking for an update...")
    } else {
        format!("Downloading item {id}...")
    };

    let reply_result = reply(channel, &text);
    wait_for_download(slf, channel, id, POLL_ATTEMPTS)?;
    reply_result
}

fn remove_item(
    slf: &Bound<'_, Workshop>,
    msg: &[String],
    channel: &Bound<'_, AbstractChannel>,
) -> PyResult<Option<i32>> {
    let Some(id) = parse_id(msg.get(2)) else {
        return usage();
    };
    let purge = match msg.get(3).map(|word| word.to_ascii_lowercase()).as_deref() {
        Some("purge") => true,
        Some("keep") => false,
        Some(_) => return usage(),
        None => cvar_text("qlx_workshopPurge", "0")? == "1",
    };
    if slf.get().is_downloading(id) {
        return reply(
            channel,
            &format!("Item {id} is still downloading, remove it afterwards."),
        );
    }
    // players could not connect any more once the files they are checked against are gone
    if is_in_use(slf, id)? {
        return reply(
            channel,
            &format!("Item {id} is in use by the running game and cannot be removed."),
        );
    }

    let listed = update_list(|list| list.remove(id))?;
    let mut text = match (listed, purge) {
        (false, _) => format!("Item {id} is not in the list."),
        (true, true) => format!("Item {id} is removed from the list."),
        (true, false) => format!(
            "Item {id} is removed from the list and leaves the server with the next restart."
        ),
    };

    if purge {
        text.push(' ');
        text.push_str(&purge_item(slf.get(), id)?);
    }
    reply(channel, &text)
}

/// Whether the running game tells its players to have the item.
fn is_in_use(plugin: &Bound<'_, Workshop>, id: u64) -> PyResult<bool> {
    let Some(game) = plugin.as_super().get_game() else {
        return Ok(false);
    };
    Ok(Bound::new(plugin.py(), game)?
        .get_workshop_items()?
        .contains(&id))
}

fn purge_item(plugin: &Workshop, id: u64) -> PyResult<String> {
    let Some(root) = content_root()? else {
        return Ok("The workshop directory was not found, so no files were deleted.".to_owned());
    };
    let report = match item::purge(&root, id) {
        Ok(report) => report,
        Err(error) => return Ok(format!("The files could not be deleted: {error}")),
    };
    // a deleted pk3 that stays in the search path stops the server when one of its maps loads
    if plugin
        .steam
        .state(id)
        .is_some_and(|state| state.is_installed())
    {
        mount(id)?;
    }
    loader::request_refresh();
    Ok(format!(
        "Deleted its files ({} manifest entries).",
        report.manifest_blocks
    ))
}

fn list_items(msg: &[String], channel: &Bound<'_, AbstractChannel>) -> PyResult<Option<i32>> {
    let files = list_files()?;
    let list = read_list(&files)?;
    let Some(root) = content_root()? else {
        return reply(channel, "The workshop directory was not found.");
    };

    let (installed, missing): (Vec<_>, Vec<_>) = list
        .entries()
        .partition(|(id, _)| item::pk3_count(&root, *id) > 0);
    let pk3_total = installed
        .iter()
        .map(|(id, _)| item::pk3_count(&root, *id))
        .sum::<usize>();
    reply(
        channel,
        &format!(
            "{} items in {}: {} installed with {pk3_total} pk3 files, {} missing.",
            installed.len() + missing.len(),
            files.write.display(),
            installed.len(),
            missing.len()
        ),
    )?;

    let show_all = msg
        .get(2)
        .is_some_and(|word| word.eq_ignore_ascii_case("all"));
    let shown = if show_all { &installed } else { &Vec::new() };
    for (id, comment) in shown.iter().chain(&missing) {
        let state = if missing.contains(&(*id, *comment)) {
            " (missing)"
        } else {
            ""
        };
        reply(channel, &format!("{id}{state} {comment}"))?;
    }
    if let Some(warning) = pk3_warning(&root, &list)? {
        reply(channel, &warning)?;
    }
    Ok(None)
}

fn add_references(plugin: &Bound<'_, Workshop>) -> PyResult<()> {
    let py = plugin.py();
    let Some(game) = plugin.as_super().get_game() else {
        return Ok(());
    };
    let game = Bound::new(py, game)?;

    let present = game.get_workshop_items()?;
    let missing_references = cvar_items("qlx_workshopReferences")
        .into_iter()
        .filter(|reference| {
            !reference
                .parse::<u64>()
                .is_ok_and(|id| present.contains(&id))
        });
    let workshop_items = present
        .iter()
        .map(u64::to_string)
        .chain(missing_references)
        .collect::<Vec<_>>();
    game.set_workshop_items(&PyList::new(py, workshop_items)?.into_any())
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, Plugin>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Workshop::new()),
    )?;
    register_hook(
        plugin.as_super(),
        EventName::Map,
        "handle_map",
        CommandPriorities::PRI_NORMAL,
    )?;
    register_hook(
        plugin.as_super(),
        EventName::ConsolePrint,
        "handle_console_print",
        CommandPriorities::PRI_NORMAL,
    )?;
    create_cvar(py, "qlx_workshopReferences", "")?;
    create_cvar(py, "qlx_workshopFile", "")?;
    create_cvar(py, "qlx_workshopPurge", "0")?;
    create_cvar(py, "qlx_workshopWarnPk3", "350")?;
    create_cvar(py, "qlx_workshopPermission", "5")?;
    let permission = cvar_text("qlx_workshopPermission", "5")?
        .parse::<i32>()
        .unwrap_or(5);
    register_command(
        plugin.as_super(),
        &["workshop"],
        "cmd_workshop",
        permission,
        "<add|remove|list> [id] [purge|keep|all]",
        None,
    )?;
    add_references(&plugin)?;

    Ok(plugin.into_super())
}

#[cfg(test)]
mod workshop_tests {
    use alloc::sync::Arc;
    use core::{
        borrow::BorrowMut,
        sync::atomic::{AtomicU32, Ordering},
    };

    use mockall::predicate;
    use pyo3::{prelude::*, types::PyTuple};
    use rstest::rstest;

    use super::{
        Workshop, WorkshopList, add_references, parse_id, pk3_warning, purge_item, update_list,
    };
    use crate::{
        engine::prelude::{CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS, CVar, CVarBuilder, cvar_t},
        prelude::*,
        python::{
            AbstractChannel, Plugin, PythonReturnCodes,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{
                drop_pending_frame_tasks, recorded_replies, recording_channel,
            },
        },
        workshop::steam::{ItemState, Progress, SteamWorkshop},
    };

    fn workshop_plugin(py: Python<'_>) -> Bound<'_, Workshop> {
        plugin_with(py, Workshop::new())
    }

    fn plugin_with(py: Python<'_>, workshop: Workshop) -> Bound<'_, Workshop> {
        Bound::new(
            py,
            PyClassInitializer::from(Plugin::py_new(py)).add_subclass(workshop),
        )
        .expect("could not create plugin")
    }

    #[derive(Clone, Default)]
    struct FakeSteam {
        state: Arc<AtomicU32>,
        downloads: Arc<AtomicU32>,
    }

    impl SteamWorkshop for FakeSteam {
        fn state(&self, _id: u64) -> Option<ItemState> {
            Some(ItemState(self.state.load(Ordering::Relaxed)))
        }

        fn progress(&self, _id: u64) -> Option<Progress> {
            Some(Progress {
                downloaded: 1024 * 1024,
                total: 4 * 1024 * 1024,
            })
        }

        fn download(&self, _id: u64) -> bool {
            self.downloads.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    const INSTALLED: u32 = 4;
    const NEEDS_UPDATE: u32 = 8;
    const DOWNLOADING: u32 = 16;
    const QUEUED: u32 = 32;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_running_download_is_described_and_not_given_up(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let steam = FakeSteam::default();
                let plugin = plugin_with(py, Workshop::with_steam(Box::new(steam.clone())));
                let channel = recording_channel(py);

                command(&plugin, &channel, &["workshop", "add", "99"]);
                steam.state.store(DOWNLOADING, Ordering::Relaxed);
                Workshop::check_download(&plugin, &channel, 99, 0).expect("waits");
                command(&plugin, &channel, &["workshop", "add", "99"]);
                steam.state.store(QUEUED, Ordering::Relaxed);
                command(&plugin, &channel, &["workshop", "add", "99"]);
                command(&plugin, &channel, &["workshop", "remove", "99"]);

                assert_eq!(steam.downloads.load(Ordering::Relaxed), 1);
                assert_eq!(
                    recorded_replies(&channel),
                    vec![
                        "Downloading item 99...",
                        "Item 99 is still downloading: 25 % (1 of 4 MB).",
                        "Item 99 is still downloading, Steam has queued it.",
                        "Item 99 is still downloading, remove it afterwards.",
                    ]
                );
                drop_pending_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_refuses_an_item_that_the_running_game_uses(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        let list = write_list_file(&home, "7\n");
        let root = install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| {
                engine
                    .with_get_configstring(CS_SERVERINFO as u16, "\\mapname\\x", 1..)
                    .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "5 7", 2)
            },
            || {
                Python::attach(|py| {
                    let plugin = workshop_plugin(py);
                    let channel = recording_channel(py);

                    command(&plugin, &channel, &["workshop", "remove", "7"]);
                    command(&plugin, &channel, &["workshop", "remove", "7", "keep"]);

                    let refusal = "Item 7 is in use by the running game and cannot be removed.";
                    assert_eq!(recorded_replies(&channel), vec![refusal, refusal]);
                });
            },
        );

        assert_eq!(std::fs::read_to_string(list).expect("list"), "7\n");
        assert!(root.join("7").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn purging_an_item_that_steam_regards_as_installed_mounts_it_anew(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        let root = install_item(&base, 42, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| engine.with_execute_console_command("steam_downloadugc 42", 1),
            || {
                Python::attach(|py| {
                    let steam = FakeSteam::default();
                    steam.state.store(INSTALLED, Ordering::Relaxed);
                    let plugin = plugin_with(py, Workshop::with_steam(Box::new(steam)));

                    purge_item(plugin.get(), 42).expect("answer");
                });
            },
        );

        assert!(!root.join("42").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_failure_line_of_the_engine_ends_the_download_with_its_code(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| engine.with_execute_console_command("steam_downloadugc 99", 2),
            || {
                Python::attach(|py| {
                    let plugin = workshop_plugin(py);
                    let channel = recording_channel(py);

                    command(&plugin, &channel, &["workshop", "add", "99"]);
                    plugin
                        .get()
                        .handle_console_print("Download item 99 failed with EResult code 9\n");
                    Workshop::check_download(&plugin, &channel, 99, 100).expect("failed");
                    command(&plugin, &channel, &["workshop", "add", "99"]);

                    assert_eq!(
                        recorded_replies(&channel),
                        vec![
                            "Downloading item 99...",
                            "The download of item 99 failed with EResult code 9.",
                            "Downloading item 99...",
                        ]
                    );
                    drop_pending_frame_tasks(py);
                });
            },
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_checks_an_item_on_the_disk_for_an_update_and_installs_it_afterwards(
        _pyshinqlx_setup: (),
    ) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let steam = FakeSteam::default();
                steam.state.store(INSTALLED | QUEUED, Ordering::Relaxed);
                let plugin = plugin_with(py, Workshop::with_steam(Box::new(steam.clone())));
                let channel = recording_channel(py);

                command(&plugin, &channel, &["workshop", "add", "7"]);
                Workshop::check_download(&plugin, &channel, 7, 3).expect("waits");
                assert_eq!(
                    recorded_replies(&channel),
                    vec!["Item 7 is on the disk, checking for an update..."]
                );

                steam.state.store(INSTALLED, Ordering::Relaxed);
                Workshop::check_download(&plugin, &channel, 7, 2).expect("done");

                assert_eq!(steam.downloads.load(Ordering::Relaxed), 1);
                let replies = recorded_replies(&channel);
                assert!(replies[1].starts_with("Item 7 is installed"), "{replies:?}");
                drop_pending_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[case::the_old_version_is_complete(INSTALLED, 1, 2)]
    #[case::steam_dropped_the_old_version(NEEDS_UPDATE, 0, 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_failed_update_installs_only_a_complete_version(
        _pyshinqlx_setup: (),
        #[case] state: u32,
        #[case] mounts: usize,
        #[case] expected_replies: usize,
    ) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| engine.with_execute_console_command("steam_downloadugc 7", mounts),
            || {
                Python::attach(|py| {
                    let steam = FakeSteam::default();
                    steam.state.store(state, Ordering::Relaxed);
                    let plugin = plugin_with(py, Workshop::with_steam(Box::new(steam)));
                    let channel = recording_channel(py);

                    command(&plugin, &channel, &["workshop", "add", "7"]);
                    plugin
                        .get()
                        .handle_console_print("Download item 7 failed with EResult code 2");
                    Workshop::check_download(&plugin, &channel, 7, 3).expect("failed");

                    let replies = recorded_replies(&channel);
                    assert_eq!(
                        replies[1],
                        "The download of item 7 failed with EResult code 2."
                    );
                    assert_eq!(replies.len(), expected_replies + 1, "{replies:?}");
                    assert!(
                        replies[2..]
                            .iter()
                            .all(|reply| reply.starts_with("Item 7 is installed")),
                        "{replies:?}"
                    );
                    drop_pending_frame_tasks(py);
                });
            },
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_appends_the_referenced_workshop_items(_pyshinqlx_setup: ()) {
        let references = c"9999,8888";
        let mut raw_references_cvar = CVarBuilder::default()
            .string(references.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "\\mapname\\campgrounds", 1..)
            .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "1234 5678", 1)
            .with_find_cvar(
                |cmd| cmd == "qlx_workshopReferences",
                move |_| CVar::try_from(raw_references_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::eq("1234 5678 9999 8888"),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let result =
                        Workshop::handle_map(&workshop_plugin(py), &PyTuple::empty(py), None);

                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_does_not_add_items_that_are_already_referenced(_pyshinqlx_setup: ()) {
        let references = c"5678, 9999";
        let mut raw_references_cvar = CVarBuilder::default()
            .string(references.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "\\mapname\\campgrounds", 1..)
            .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "1234 5678", 1)
            .with_find_cvar(
                |cmd| cmd == "qlx_workshopReferences",
                move |_| CVar::try_from(raw_references_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::eq("1234 5678 9999"),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let result = add_references(&workshop_plugin(py));

                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_without_a_game_does_nothing(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1..)
            .run(|| {
                Python::attach(|py| {
                    let result =
                        Workshop::handle_map(&workshop_plugin(py), &PyTuple::empty(py), None);

                    assert!(result.is_ok());
                });
            });
    }

    fn leaked_cvar(value: &str) -> *mut cvar_t {
        let text = alloc::ffi::CString::new(value)
            .expect("no NUL in the test value")
            .into_raw();
        let raw = Box::leak(Box::new(
            CVarBuilder::default()
                .string(text)
                .build()
                .expect("this should not happen"),
        ));
        ptr::from_mut(raw)
    }

    fn without_a_game(values: &[(&'static str, String)], test: impl FnOnce()) {
        with_engine(
            values,
            |engine| engine.with_get_configstring(CS_SERVERINFO as u16, "", 1..),
            test,
        );
    }

    fn with_cvars(values: &[(&'static str, String)], test: impl FnOnce()) {
        with_engine(values, |builder| builder, test);
    }

    fn with_engine(
        values: &[(&'static str, String)],
        configure: impl FnOnce(MockEngineBuilder) -> MockEngineBuilder,
        test: impl FnOnce(),
    ) {
        let mut builder = MockEngineBuilder::default();
        for (name, value) in values {
            let (name, cvar) = (*name, leaked_cvar(value));
            builder = builder.with_find_cvar(
                move |cmd| cmd == name,
                move |_| CVar::try_from(cvar).ok(),
                0..,
            );
        }
        configure(builder)
            .with_find_cvar(|_| true, |_| None, 0..)
            .run(test);
    }

    fn command(
        plugin: &Bound<'_, Workshop>,
        channel: &Bound<'_, AbstractChannel>,
        words: &[&str],
    ) -> Option<i32> {
        let py = plugin.py();
        Workshop::cmd_workshop(
            plugin,
            &py.None().into_bound(py),
            words.iter().map(|word| (*word).to_owned()).collect(),
            channel,
        )
        .expect("the command runs")
    }

    fn write_list_file(home: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
        let path = home.path().join("baseq3/workshop.txt");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, content).expect("list file");
        path
    }

    fn directory_cvars(
        home: &tempfile::TempDir,
        base: &tempfile::TempDir,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("fs_homepath", home.path().to_string_lossy().into_owned()),
            ("fs_basepath", base.path().to_string_lossy().into_owned()),
        ]
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn update_list_copies_the_list_of_the_base_path_into_the_home_path(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        std::fs::create_dir_all(base.path().join("baseq3")).expect("dirs");
        std::fs::write(
            base.path().join("baseq3/workshop.txt"),
            "# list\n1 # a.pk3 (a)\n",
        )
        .expect("file");

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|_py| {
                let changed = update_list(|list| list.add(2, "b.pk3 (b)"));
                assert!(changed.is_ok_and(|changed| changed));
            });
        });

        assert_eq!(
            std::fs::read_to_string(home.path().join("baseq3/workshop.txt")).expect("written"),
            "# list\n1 # a.pk3 (a)\n2 # b.pk3 (b)\n"
        );
        assert_eq!(
            std::fs::read_to_string(base.path().join("baseq3/workshop.txt")).expect("untouched"),
            "# list\n1 # a.pk3 (a)\n"
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn update_list_writes_nothing_when_the_list_does_not_change(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|_py| {
                let changed = update_list(|list| list.remove(7));
                assert!(changed.is_ok_and(|changed| !changed));
            });
        });

        assert!(!home.path().join("baseq3/workshop.txt").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_override_file_is_used_instead_of_the_home_path(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        let shared = base.path().join("shared.txt");
        let mut cvars = directory_cvars(&home, &base);
        cvars.push(("qlx_workshopFile", shared.to_string_lossy().into_owned()));

        with_cvars(&cvars, || {
            Python::attach(|_py| {
                assert!(update_list(|list| list.add(5, "")).is_ok_and(|changed| changed));
            });
        });

        assert_eq!(std::fs::read_to_string(&shared).expect("written"), "5\n");
        assert!(!home.path().join("baseq3/workshop.txt").exists());
    }

    fn install_item(base: &tempfile::TempDir, id: u64, pk3s: &[&str]) -> std::path::PathBuf {
        let dir = base
            .path()
            .join("steamapps/workshop/content/282440")
            .join(id.to_string());
        std::fs::create_dir_all(&dir).expect("item dir");
        for name in pk3s {
            let mut writer = zip::ZipWriter::new(
                std::fs::File::create(dir.join(name)).expect("pk3 can be created"),
            );
            writer
                .start_file("maps/x.bsp", zip::write::SimpleFileOptions::default())
                .expect("entry");
            writer.finish().expect("pk3");
        }
        base.path().join("steamapps/workshop/content/282440")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_pk3_warning_appears_only_above_the_limit(_pyshinqlx_setup: ()) {
        let base = tempfile::tempdir().expect("temp dir");
        let root = install_item(&base, 1, &["a.pk3", "b.pk3"]);
        install_item(&base, 2, &["c.pk3"]);
        let list = WorkshopList::parse("1\n2\n");

        for (limit, warned) in [("0", false), ("3", false), ("2", true)] {
            with_cvars(&[("qlx_workshopWarnPk3", limit.to_owned())], || {
                Python::attach(|_py| {
                    let warning = pk3_warning(&root, &list).expect("readable");
                    assert_eq!(warning.is_some(), warned, "limit {limit}");
                    if let Some(text) = warning {
                        assert!(text.contains("3 pk3 files, more than 2"));
                    }
                });
            });
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn purge_item_deletes_the_files(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        let root = install_item(&base, 42, &["a.pk3"]);

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let text = purge_item(plugin.get(), 42).expect("answer");

                assert!(text.starts_with("Deleted its files"), "{text}");
            });
        });

        assert!(!root.join("42").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn purge_item_without_a_content_directory_says_so(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let text = purge_item(plugin.get(), 42).expect("answer");

                assert!(text.contains("no files were deleted"), "{text}");
            });
        });
    }

    #[rstest]
    #[case(Some("42"), Some(42))]
    #[case(Some("0"), None)]
    #[case(Some("abc"), None)]
    #[case(Some("-5"), None)]
    #[case(None, None)]
    fn ids_are_positive_numbers(#[case] text: Option<&str>, #[case] expected: Option<u64>) {
        assert_eq!(parse_id(text.map(str::to_owned).as_ref()), expected);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_mounts_and_lists_an_item_that_is_already_downloaded(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| engine.with_execute_console_command("steam_downloadugc 7", 1),
            || {
                Python::attach(|py| {
                    let plugin = workshop_plugin(py);
                    let channel = recording_channel(py);

                    assert_eq!(command(&plugin, &channel, &["workshop", "add", "7"]), None);

                    let replies = recorded_replies(&channel);
                    assert!(
                        replies[0].starts_with("Item 7 is installed: "),
                        "{replies:?}"
                    );
                    assert_eq!(replies.len(), 1);
                });
            },
        );

        let list = std::fs::read_to_string(home.path().join("baseq3/workshop.txt")).expect("list");
        assert!(list.starts_with("7 # "), "{list}");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_downloads_an_item_that_is_missing_and_waits_for_it(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| engine.with_execute_console_command("steam_downloadugc 99", 1),
            || {
                Python::attach(|py| {
                    let plugin = workshop_plugin(py);
                    let channel = recording_channel(py);

                    command(&plugin, &channel, &["workshop", "add", "99"]);

                    assert_eq!(recorded_replies(&channel), vec!["Downloading item 99..."]);
                    drop_pending_frame_tasks(py);
                });
            },
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_does_not_restart_a_running_download_until_it_is_given_up(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 7, &["a.pk3"]);

        with_engine(
            &directory_cvars(&home, &base),
            |engine| {
                engine
                    .with_execute_console_command("steam_downloadugc 99", 2)
                    .with_execute_console_command("steam_downloadugc 100", 1)
            },
            || {
                Python::attach(|py| {
                    let plugin = workshop_plugin(py);
                    let channel = recording_channel(py);

                    command(&plugin, &channel, &["workshop", "add", "99"]);
                    command(&plugin, &channel, &["workshop", "add", "99"]);
                    command(&plugin, &channel, &["workshop", "add", "100"]);
                    Workshop::check_download(&plugin, &channel, 99, 1).expect("waits");
                    command(&plugin, &channel, &["workshop", "add", "99"]);
                    Workshop::check_download(&plugin, &channel, 99, 0).expect("gave up");
                    command(&plugin, &channel, &["workshop", "add", "99"]);

                    assert_eq!(
                        recorded_replies(&channel),
                        vec![
                            "Downloading item 99...",
                            "Item 99 is still downloading.",
                            "Downloading item 100...",
                            "Item 99 is still downloading.",
                            "Item 99 did not arrive, see the server log for the download result.",
                            "Downloading item 99...",
                        ]
                    );
                    drop_pending_frame_tasks(py);
                });
            },
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_says_when_the_workshop_directory_is_missing(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = recording_channel(py);

                command(&plugin, &channel, &["workshop", "add", "5"]);

                assert_eq!(
                    recorded_replies(&channel),
                    vec!["The workshop directory was not found."]
                );
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_download_that_arrives_is_listed_and_one_that_does_not_is_given_up(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        install_item(&base, 8, &["a.pk3"]);

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = recording_channel(py);

                Workshop::check_download(&plugin, &channel, 8, 3).expect("arrived");
                Workshop::check_download(&plugin, &channel, 9, 0).expect("gave up");
                Workshop::check_download(&plugin, &channel, 9, 2).expect("waits");

                let replies = recorded_replies(&channel);
                assert!(replies[0].starts_with("Item 8 is installed"), "{replies:?}");
                assert!(replies[1].contains("Item 9 did not arrive"), "{replies:?}");
                assert_eq!(replies.len(), 2);
                drop_pending_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_takes_an_item_from_the_list_and_purges_only_when_asked(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        let list = write_list_file(&home, "3 # c.pk3\n4 # d.pk3\n5\n");
        let root = install_item(&base, 4, &["d.pk3"]);

        without_a_game(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = recording_channel(py);

                command(&plugin, &channel, &["workshop", "remove", "3"]);
                command(&plugin, &channel, &["workshop", "remove", "3", "keep"]);
                command(&plugin, &channel, &["workshop", "remove", "4", "PURGE"]);

                let replies = recorded_replies(&channel);
                assert!(replies[0].starts_with("Item 3 is removed"), "{replies:?}");
                assert_eq!(replies[1], "Item 3 is not in the list.");
                assert!(replies[2].contains("Deleted its files"), "{replies:?}");
            });
        });

        assert_eq!(std::fs::read_to_string(list).expect("list"), "5\n");
        assert!(!root.join("4").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_follows_the_cvar_for_purging_and_refuses_unknown_words(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        write_list_file(&home, "3\n");
        let root = install_item(&base, 3, &["c.pk3"]);
        let mut cvars = directory_cvars(&home, &base);
        cvars.push(("qlx_workshopPurge", "1".to_owned()));

        without_a_game(&cvars, || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = recording_channel(py);

                let unknown = command(&plugin, &channel, &["workshop", "remove", "3", "maybe"]);
                let no_id = command(&plugin, &channel, &["workshop", "remove"]);
                command(&plugin, &channel, &["workshop", "remove", "3"]);

                let usage = Some(PythonReturnCodes::RET_USAGE as i32);
                assert_eq!((unknown, no_id), (usage, usage));
                let replies = recorded_replies(&channel);
                assert!(replies[0].contains("Deleted its files"), "{replies:?}");
            });
        });

        assert!(!root.join("3").exists());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn list_counts_the_installed_and_missing_items_and_names_them_on_request(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );
        write_list_file(&home, "1 # a.pk3\n2 # gone\n");
        install_item(&base, 1, &["a.pk3", "b.pk3"]);
        let mut cvars = directory_cvars(&home, &base);
        cvars.push(("qlx_workshopWarnPk3", "1".to_owned()));

        with_cvars(&cvars, || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let short = recording_channel(py);
                let full = recording_channel(py);

                command(&plugin, &short, &["workshop", "list"]);
                command(&plugin, &full, &["workshop", "list", "ALL"]);

                let short = recorded_replies(&short);
                assert!(
                    short[0].contains("2 items in")
                        && short[0].contains("1 installed with 2 pk3 files, 1 missing."),
                    "{short:?}"
                );
                assert_eq!(short[1], "2 (missing) gone");
                assert!(short[2].contains("2 pk3 files, more than 1"), "{short:?}");
                let full = recorded_replies(&full);
                assert_eq!(full[1], "1 a.pk3");
                assert_eq!(full[2], "2 (missing) gone");
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn list_without_the_workshop_directory_says_so(_pyshinqlx_setup: ()) {
        let (home, base) = (
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        );

        with_cvars(&directory_cvars(&home, &base), || {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = recording_channel(py);

                command(&plugin, &channel, &["workshop", "list"]);

                assert_eq!(
                    recorded_replies(&channel),
                    vec!["The workshop directory was not found."]
                );
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_unknown_subcommand_answers_with_the_usage(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let plugin = workshop_plugin(py);
                let channel = super::super::test_support::named_channel(py, "test");

                for words in [
                    vec!["workshop"],
                    vec!["workshop", "frobnicate"],
                    vec!["workshop", "add"],
                ] {
                    let result = Workshop::cmd_workshop(
                        &plugin,
                        &py.None().into_bound(py),
                        words.into_iter().map(str::to_owned).collect(),
                        &channel,
                    );
                    assert!(
                        result.is_ok_and(|code| code == Some(PythonReturnCodes::RET_USAGE as i32))
                    );
                }
            });
        });
    }
}
